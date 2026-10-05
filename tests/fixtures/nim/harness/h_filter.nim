import std/[json, strutils, re]

let
  words = readFile("/home/ubuntu/github-repo/markinim/src/premium/bad-words.csv").strip(chars = {' ', '\n', '\r'})
  SfwRegex = re(words.split("\n").join("|"), flags = {reIgnoreCase, reStudy})

  UrlRegex = re(r"""(?i)\b((?:https?://|www\d{0,3}[.]|[a-z0-9.\-]+[.][a-z]{2,4}/)(?:[^\s()<>]+|\(([^\s()<>]+|(\([^\s()<>]+\)))*\))+(?:\(([^\s()<>]+|(\([^\s()<>]+\)))*\)|[^\s`!()\[\]{};:'\".,<>?«»“”‘’]))""", flags = {reIgnoreCase, reStudy})
  UsernameRegex = re("@([a-zA-Z](_(?!_)|[a-zA-Z0-9]){3,32}[a-zA-Z0-9])", flags = {reIgnoreCase, reStudy})

for line in stdin.lines:
  if line == "": continue
  let s = parseJson(line).getStr()
  let blank = s.strip() == ""
  echo $(%*{"blank": blank, "sfw": s.find(SfwRegex) != -1, "url": s.find(UrlRegex) != -1, "user": s.find(UsernameRegex) != -1})
