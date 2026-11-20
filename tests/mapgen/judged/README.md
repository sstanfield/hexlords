# Judged maps

Terrain the generator made at milestone G2 (`mapgen batch --seeds 1..16 --size
medium`), judged by the owner (2026-11-20). They are kept as they were made,
so that measures can be held to the owner's eye even after the generator has
changed (`tests/mapgen_taste.rs`). `mapgen.txt` is the config each was made
with, at commit 0e8fc3f.

- Best (subjective): 1, 9, 12, 13, 16. The mountain-ringed inland lake of 1
  was liked; the long range of 13 was the best; of the clumps, 9's.
- Worst: 2, 3, 8, 11, 15. The small space shut in by mountains in 11 looked
  odd and wastes the map (1's mountain lake only a little less).
- None was a show stopper.

What came of it: land walled in by mountains is measured (`mapgen stats`:
"walled in by mountains"; 410 and 196 hexes on 3 and 11, at most 25 on the
best), and the generator opens such pockets (a saddle cut into hills) or
fills a hollow of a few hexes; the ranges are spaced wider, bent more, and
lumpier, so fewer run in straight parallel stripes (2, 8, 15).
