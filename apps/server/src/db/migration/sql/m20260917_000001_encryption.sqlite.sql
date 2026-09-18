-- End-to-end encrypted shares.
--
-- The server stores these without being able to read them. Everything it
-- would otherwise hold in the clear about the content, a file's name and
-- media type, a share's title and a note's language, arrives sealed instead,
-- as unpadded URL-safe base64 of an object in the format `docs/encryption.md`
-- describes. The server keeps it and hands it back; it never opens it.
--
-- # `encryption`
--
-- `link` when the key travels in the link after `#`, `password` when it is
-- derived from one, NULL for a share that is not encrypted. On `uploads`, what
-- the share it creates will be.
--
-- # `key_salt`
--
-- The salt a password is stretched with, for `password` only. Public by
-- design: a client needs it before it can derive the key, and so before it
-- can present the password.
--
-- # `sealed_details` and `sealed_name`
--
-- The share's title and a note's language, and a file's name and media type,
-- sealed. The plain columns beside them stay NULL on an encrypted share, so a
-- reader that knows nothing of encryption shows nothing rather than ciphertext.
--
-- TEXT, not VARCHAR(191) like the plain names: sealing adds a header and a
-- tag, base64 adds a third, and none of these is ever indexed.
ALTER TABLE shares ADD COLUMN encryption TEXT;
ALTER TABLE shares ADD COLUMN key_salt TEXT;
ALTER TABLE shares ADD COLUMN sealed_details TEXT;
ALTER TABLE file_shares ADD COLUMN sealed_name TEXT;
ALTER TABLE uploads ADD COLUMN encryption TEXT;
ALTER TABLE uploads ADD COLUMN key_salt TEXT;
ALTER TABLE uploads ADD COLUMN sealed_name TEXT;
