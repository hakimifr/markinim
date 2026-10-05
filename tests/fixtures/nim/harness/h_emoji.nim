import std/[json]
import emojipasta
for line in stdin.lines:
  if line == "": continue
  let s = parseJson(line).getStr()
  echo $(%*{"in": s, "out": emojify(s)})
