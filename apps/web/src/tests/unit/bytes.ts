/**
 * Where two byte arrays first differ, or null when they are identical. Use as
 * `expect(byteDiff(actual, expected)).toBeNull()`: `toEqual` walks a
 * Uint8Array element by element and prints a megabyte-sized diff, while this
 * compares natively and fails with one line naming the offset.
 */
export const byteDiff = (
  actual: Uint8Array,
  expected: Uint8Array
): string | null => {
  if (Buffer.compare(actual, expected) === 0) {
    return null;
  }
  const shorter = Math.min(actual.length, expected.length);
  let offset = 0;
  while (offset < shorter && actual[offset] === expected[offset]) {
    offset += 1;
  }
  return `bytes differ at offset ${offset} (actual length ${actual.length}, expected ${expected.length})`;
};
