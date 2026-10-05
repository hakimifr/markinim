# Nim's default split()/strip() whitespace semantics + toLower (ASCII) on a corpus of JSON strings, one per line.
import std/[json, strutils]
for line in stdin.lines:
  if line == "": continue
  let s = parseJson(line).getStr()
  var parts = newJArray()
  for p in s.split(): parts.add(%p)
  echo $(%*{"split": parts, "strip_empty": s.strip() == "", "lower": s.toLower()})
