import std/[json, strutils, os]
import owoifynim
let level = paramStr(1)
for line in stdin.lines:
  if line == "": continue
  let s = parseJson(line).getStr()
  echo $(%*{"in": s, "out": owoify(s, level)})
