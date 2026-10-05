import std/[unicode, strutils]
# every scalar value -> Nim's rune-wise toLower (as used by unicodeStringToLower)
for cp in 0 .. 0x10FFFF:
  if cp in 0xD800 .. 0xDFFF: continue
  let low = toLower(Rune(cp))
  if int(low) != cp:
    echo toHex(cp), " ", toHex(int(low))
