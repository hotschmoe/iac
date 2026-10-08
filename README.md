# Amber Scout

A playable [In Amber Clad](https://github.com/hotschmoe/iac) slice in Numen:
one scout fleet on a seeded hex inner ring. CLI and amber HTTP/1.0 page.

Win: reach hub `[0,0]` with at least 40 cargo after at least one fight.
Lose: no ships, or no fuel and nothing left to harvest.

```
cd amber-scout
numen test src/hex.nmn
numen test src/world.nmn
numen test src/fleet.nmn
numen test src/game.nmn
numen test src/linux_sock.nmn
numen test src/http.nmn
numen build
./numen-out/bin/amber-scout           # CLI
./numen-out/bin/amber-scout --http    # http://127.0.0.1:18080/
```

Commands: `look` `move E|NE|NW|W|SW|SE` `scan` `harvest` `fight` `help` `quit`

From a parent directory: `numen build --build-file amber-scout/build.nmn`.
`--project-root` is a direct-compile bound, not script mode (numen#17).

Seed 1 start: `[3,-1]`, two scouts, fuel 60.

Proven win path (fight after every move; some sectors are empty):

```
harvest
harvest
harvest
move nw
move ne
move e
move se
fight
move w
fight
move w
fight
move w
fight
move w
fight
move w
fight
move se
fight
move se
```
