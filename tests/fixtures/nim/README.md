# Nim parity fixtures

Expected values for `tests/nim_parity.rs`, captured by running the **original
Nim bot's code** (commit `7b08a93`) and its dependencies, so the Rust port can
be checked against what the Nim bot actually produced without needing a Nim
toolchain.

| Fixture | Produced by | Covers |
|---|---|---|
| `lowercase.txt` | `harness/h_lower.nim` | every code point Nim's `unicode.toLower` changes (`HEX HEX`) |
| `model.jsonl` | `harness/h_model.nim` (real `nimkov`) | the transition table `newMarkov(samples, asLower)` builds |
| `strings.jsonl` | `harness/h_strings.nim` | `split()`, `strip() == ""`, `toLower` |
| `owo.jsonl` | `harness/h_owo_orig.nim` (real `owoifynim`, see below) | `owoify` per level with pinned random draws |
| `emoji.jsonl` | `harness/h_emoji.nim` (real `emojipasta`, see below) | `emojify` with pinned random draws |
| `filter.jsonl` | `harness/h_filter.nim` (`std/re`) | the SFW, URL and username regexes |
| `poll.jsonl` | `harness/h_poll.nim` | `/wouldyourather` dedupe, sort and trim |

Toolchain used: Nim 2.0.2 (the version the old Dockerfile pinned) and PCRE
8.45 (`libpcre.so.3`, as `std/re` and `nre` load it). Dependency sources:
`nimkov` `0314a75`, `owoifynim` `34e1ea4`, `emojipasta` `6c51316`.

## Pinned randomness

Two libraries draw random numbers, which would make fixtures unrepeatable. For
those two runs the Nim packages were copied and only the draws replaced:

- `owoifynim/private/mapping.nim`: `rand(1) > 0` became
  `getEnv("OWO_COIN", "1") == "1"`, and `FACES[rand(len(FACES) - 1)]` became
  `FACES[parseInt(getEnv("OWO_FACE", "0"))]`.
- `emojipasta/utils.nim`: `rand(x)` became `0` (`EP_MODE=zero`), `x`
  (`EP_MODE=max`) or `x div 2` (`EP_MODE=half`).

`owo.jsonl` records the `coin` and `face` used per row, `emoji.jsonl` the
`mode`. The Rust tests feed the same values through `OwoRandom` and
`EmojiRandom`.
