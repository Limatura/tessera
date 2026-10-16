/* oxlint-disable eslint/no-bitwise -- CRC-32 and the zip format are bit fields */

/**
 * A zip written as it streams, for an encrypted share of several files: the
 * server will not pack sealed files (no archiver could open the result), so
 * the browser opens each one and packs the plain bytes itself.
 *
 * Stored, not compressed: the archive's length is then known before the first
 * byte, which the save target checks against, and most files people send are
 * compressed already. Sizes are known up front, so each local header carries
 * them; only the CRC follows in a data descriptor. ZIP64 records are added
 * only where a size or offset does not fit in 32 bits.
 */

export interface ZipEntry {
  /** `/`-separated path inside the archive. */
  name: string;
  size: number;
  /** Called when the entry is reached, so only one file is fetched at a time. */
  open: () => Promise<ReadableStream<Uint8Array>>;
}

const LIMIT = 0xff_ff_ff_ff;
const encoder = new TextEncoder();

/** The exact length of the archive {@link zipStream} writes. */
export function zipLength(entries: Omit<ZipEntry, "open">[]): number {
  let offset = 0;
  let central = 0;
  for (const entry of entries) {
    const name = encoder.encode(entry.name).length;
    const big = entry.size >= LIMIT;
    const far = offset >= LIMIT;
    central += 46 + name + centralExtraLength(big, far);
    offset += 30 + name + (big ? 20 : 0) + entry.size + (big ? 24 : 16);
  }
  return offset + central + endLength(entries.length, offset, central);
}

export function zipStream(entries: ZipEntry[]): ReadableStream<Uint8Array> {
  const records: {
    name: Uint8Array;
    size: number;
    crc: number;
    offset: number;
  }[] = [];
  let offset = 0;
  let index = 0;
  let current:
    | {
        reader: ReadableStreamDefaultReader<Uint8Array>;
        crc: number;
        read: number;
      }
    | undefined;

  return new ReadableStream<Uint8Array>({
    cancel: async () => {
      await current?.reader.cancel();
    },
    pull: async (controller) => {
      const emit = (bytes: Uint8Array) => {
        controller.enqueue(bytes);
        offset += bytes.length;
      };

      if (!current) {
        const entry = entries[index];
        if (!entry) {
          const start = offset;
          for (const record of records) {
            emit(centralHeader(record));
          }
          emit(end(records.length, start, offset - start));
          controller.close();
          return;
        }
        const name = encoder.encode(entry.name);
        records.push({ crc: 0, name, offset, size: entry.size });
        emit(localHeader(name, entry.size));
        const opened = await entry.open();
        current = { crc: CRC_START, read: 0, reader: opened.getReader() };
      }

      const { done, value } = await current.reader.read();
      const record = records.at(-1) as (typeof records)[number];
      if (!done) {
        current.read += value.length;
        if (current.read > record.size) {
          throw new Error(`${entries[index]?.name} ran long`);
        }
        current.crc = crc32(current.crc, value);
        emit(value);
        return;
      }
      if (current.read !== record.size) {
        throw new Error(`${entries[index]?.name} ended early`);
      }
      record.crc = (current.crc ^ CRC_START) >>> 0;
      emit(descriptor(record.crc, record.size));
      current = undefined;
      index += 1;
    },
  });
}

/** Bit 3: CRC in a data descriptor. Bit 11: the name is UTF-8. */
const FLAGS = 0x08_08;
const VERSION = 45;

function localHeader(name: Uint8Array, size: number) {
  const big = size >= LIMIT;
  const out = new DataView(new ArrayBuffer(30 + name.length + (big ? 20 : 0)));
  out.setUint32(0, 0x04_03_4b_50, true);
  out.setUint16(4, VERSION, true);
  out.setUint16(6, FLAGS, true);
  // Method 0 (stored), time and date 0, CRC 0 until the descriptor
  out.setUint32(18, big ? LIMIT : size, true);
  out.setUint32(22, big ? LIMIT : size, true);
  out.setUint16(26, name.length, true);
  out.setUint16(28, big ? 20 : 0, true);
  const bytes = new Uint8Array(out.buffer);
  bytes.set(name, 30);
  if (big) {
    const extra = new DataView(out.buffer, 30 + name.length);
    extra.setUint16(0, 1, true);
    extra.setUint16(2, 16, true);
    extra.setBigUint64(4, BigInt(size), true);
    extra.setBigUint64(12, BigInt(size), true);
  }
  return bytes;
}

function descriptor(crc: number, size: number) {
  const big = size >= LIMIT;
  const out = new DataView(new ArrayBuffer(big ? 24 : 16));
  out.setUint32(0, 0x08_07_4b_50, true);
  out.setUint32(4, crc, true);
  if (big) {
    out.setBigUint64(8, BigInt(size), true);
    out.setBigUint64(16, BigInt(size), true);
  } else {
    out.setUint32(8, size, true);
    out.setUint32(12, size, true);
  }
  return new Uint8Array(out.buffer);
}

function centralExtraLength(big: boolean, far: boolean) {
  const fields = (big ? 16 : 0) + (far ? 8 : 0);
  return fields === 0 ? 0 : 4 + fields;
}

function centralHeader(record: {
  name: Uint8Array;
  size: number;
  crc: number;
  offset: number;
}) {
  const big = record.size >= LIMIT;
  const far = record.offset >= LIMIT;
  const extra = centralExtraLength(big, far);
  const out = new DataView(new ArrayBuffer(46 + record.name.length + extra));
  out.setUint32(0, 0x02_01_4b_50, true);
  out.setUint16(4, VERSION, true);
  out.setUint16(6, VERSION, true);
  out.setUint16(8, FLAGS, true);
  out.setUint32(16, record.crc, true);
  out.setUint32(20, big ? LIMIT : record.size, true);
  out.setUint32(24, big ? LIMIT : record.size, true);
  out.setUint16(28, record.name.length, true);
  out.setUint16(30, extra, true);
  out.setUint32(42, far ? LIMIT : record.offset, true);
  const bytes = new Uint8Array(out.buffer);
  bytes.set(record.name, 46);
  if (extra > 0) {
    let at = 46 + record.name.length;
    out.setUint16(at, 1, true);
    out.setUint16(at + 2, extra - 4, true);
    at += 4;
    if (big) {
      out.setBigUint64(at, BigInt(record.size), true);
      out.setBigUint64(at + 8, BigInt(record.size), true);
      at += 16;
    }
    if (far) {
      out.setBigUint64(at, BigInt(record.offset), true);
    }
  }
  return bytes;
}

function needsZip64(count: number, start: number, length: number) {
  return count >= 0xff_ff || start >= LIMIT || length >= LIMIT;
}

function endLength(count: number, start: number, length: number) {
  return 22 + (needsZip64(count, start, length) ? 56 + 20 : 0);
}

function end(count: number, start: number, length: number) {
  const zip64 = needsZip64(count, start, length);
  const out = new DataView(new ArrayBuffer(endLength(count, start, length)));
  let at = 0;
  if (zip64) {
    out.setUint32(0, 0x06_06_4b_50, true);
    out.setBigUint64(4, 44n, true);
    out.setUint16(12, VERSION, true);
    out.setUint16(14, VERSION, true);
    out.setBigUint64(24, BigInt(count), true);
    out.setBigUint64(32, BigInt(count), true);
    out.setBigUint64(40, BigInt(length), true);
    out.setBigUint64(48, BigInt(start), true);
    out.setUint32(56, 0x07_06_4b_50, true);
    out.setBigUint64(64, BigInt(start + length), true);
    out.setUint32(72, 1, true);
    at = 76;
  }
  out.setUint32(at, 0x06_05_4b_50, true);
  out.setUint16(at + 8, Math.min(count, 0xff_ff), true);
  out.setUint16(at + 10, Math.min(count, 0xff_ff), true);
  out.setUint32(at + 12, Math.min(length, LIMIT), true);
  out.setUint32(at + 16, Math.min(start, LIMIT), true);
  return new Uint8Array(out.buffer);
}

const CRC_START = 0xff_ff_ff_ff;
const CRC_TABLE = Uint32Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) {
    c = c & 1 ? 0xed_b8_83_20 ^ (c >>> 1) : c >>> 1;
  }
  return c >>> 0;
});

function crc32(crc: number, bytes: Uint8Array) {
  let c = crc;
  for (const byte of bytes) {
    c = (CRC_TABLE[(c ^ byte) & 0xff] as number) ^ (c >>> 8);
  }
  return c >>> 0;
}
