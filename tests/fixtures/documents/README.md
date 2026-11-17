# Original PDF fixtures

Project-generated minimal PDF 1.7 files, GPL-3.0-only. No external document or font program is embedded.

- `text-pages.pdf`: three pages; pages one and three contain `猫です。` using a Type0 font and an explicit ToUnicode CMap; page two is empty.
- `no-text.pdf`: the same page tree with empty content streams.
- `password.pdf`: the same page tree behind a Standard revision-2 encryption dictionary with fixed synthetic owner/user values. It is intended only to exercise password rejection, never successful decryption.

All files have computed object offsets, a cross-reference table and a trailer. Tests use the installed Poppler utility, without downloading a PDF library or running OCR.
