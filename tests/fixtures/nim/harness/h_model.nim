# Reads JSON lines {"as_lower":bool,"samples":[...]} and dumps the nimkov model.
import std/[json, tables, algorithm, strutils]
import nimkov/[generator, objects, typedefs, constants]

for line in stdin.lines:
  if line.strip() == "": continue
  let j = parseJson(line)
  var samples: seq[string]
  for s in j["samples"]: samples.add(s.getStr())
  let m = newMarkov(samples, asLower = j["as_lower"].getBool())
  var rows: seq[(string, string, int)]
  for a, followers in m.model.pairs:
    for b, c in followers.pairs:
      rows.add((a, b, c))
  rows.sort()
  var arr = newJArray()
  for (a, b, c) in rows: arr.add(%*[a, b, c])
  var sm = newJArray()
  for s in m.samples: sm.add(%s)
  echo $(%*{"samples": sm, "model": arr})
