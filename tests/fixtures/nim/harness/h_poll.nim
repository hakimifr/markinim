import std/[json, algorithm, sequtils]
from std / unicode import runeOffset
proc byLength(a, b: string): int = cmp(len(a), len(b))
proc trimUnicode(s: string, length: int): string =
  let offset = s.runeOffset(length)
  if offset == -1:
    return s
  return s[0 ..< offset]
proc sortCandidates(options: seq[string], length: int): seq[string] =
  var options = options
  # let
  #   lengths = options.mapIt(it.len)
  #   minLength = min(lengths)
  #   maxLength = max(lengths)

  options.sort(byLength)
  options.reverse()

  for i in 0 ..< options.len:
    if options[i].len > length:
      options[i] = options[i].trimUnicode(length)

  return options

for line in stdin.lines:
  if line == "": continue
  var opts: seq[string]
  for s in parseJson(line): opts.add(s.getStr())
  var r = opts.deduplicate(isSorted = false)
  r = r.sortCandidates(length = 100)
  var arr = newJArray()
  for s in r: arr.add(%s)
  echo $arr
