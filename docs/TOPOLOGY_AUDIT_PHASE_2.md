# Topology Audit — Phase 2 Findings

**Status:** Experimental/offline diagnostics only. These findings do not change the playable world's topology or rendering.

## Scope

The current comparison evaluates two candidate spherical surface meshes:

- A cube-sphere surface partition.
- The dual polygon mesh of a subdivided icosahedron (geodesic dual).

The goal is to measure geometry and topology before choosing a surface partition compatible with Hexel Engine's pure hexagonal-prism gameplay cells.

## Recorded geometry results

Measurements below use radius 100 and the settings recorded during the Phase 2 audit.

| Metric | Cube-sphere (resolution 4) | Geodesic dual (frequency 3) |
|---|---:|---:|
| Cells | 96 | 92 |
| Surface vertices | 98 | 180 |
| Unique edges | 192 | 270 |
| Cell side distribution | 96 quads | 12 pentagons, 80 hexagons |
| Area coefficient of variation | 0.333649 | 0.145563 |
| Edge length min / mean / max | 27.477100 / 35.545705 / 45.950584 | 21.915211 / 23.197045 / 24.482140 |
| Cell area min / mean / max | 814.555876 / 1308.996939 / 2013.579208 | 866.726518 / 1365.909849 / 1526.991978 |

Both audited meshes were closed in these runs, with zero boundary edges, zero non-manifold edges, and zero measured radius error.

## Current automated checks

The local full test run completed successfully:

- Core engine: 46 tests passed.
- Cube-sphere geometry checker: 8 tests passed.
- Geodesic dual geometry checker: 13 tests passed.
- **Total: 67 passed, 0 failed.**

The geodesic checker includes checks for expected frequency-2 and frequency-3 counts, pentagon/hexagon distribution, deterministic generation, invalid inputs, degenerate cells, duplicate cell IDs, edge-use counts, shared-edge traversal in opposite directions, total spherical area, and positive finite signed spherical area for generated cells.

The cube-sphere checker includes expected mesh counts, quad loops, edge sharing, radius, positive cell areas, and total spherical area.

These are the results of the local working tree at the time of this report. The topology checker source had local modifications during this audit; verify the working tree and run the commands below when synchronizing.

## Interpretation

- **Area uniformity:** the geodesic dual has lower measured cell-area variation than the tested cube-sphere mesh at these specific settings. This is a comparison result, not proof that the geodesic partition is the final design.
- **Edge lengths:** the geodesic dual's measured edge lengths are more uniform in this comparison.
- **Cell shape compatibility:** the geodesic dual provides mostly hexagons, with the expected 12 pentagons. The cube-sphere candidate produces quads, which do not directly satisfy a six-sided horizontal-cell requirement.
- **Not yet proven:** positive signed area and total-area agreement do not prove global non-overlap or complete geometric coverage. These need separate robust validation.
- **Gameplay unchanged:** neither experimental mesh is connected to or alters the playable world's current topology.

## Remaining validation work

1. Derive cell adjacency from shared edges and test reciprocal neighbor relationships.
2. Strengthen global coverage and overlap validation.
3. Decide how the 12 pentagonal cells of a geodesic dual could be represented while preserving Hexel's pure hexagonal-prism requirement; do not silently substitute arbitrary polyhedra.
4. Test prism wall construction for cracks, overlaps, and inconsistent shared boundaries.
5. Keep deterministic IDs and neighbor reciprocity as explicit invariants when adjacency data is designed.

## Reproducible commands

Run from the repository root:

```sh
cargo test --bin cube_sphere_geometry_check
cargo test --bin topology_geometry_check
cargo test
```

**Decision at this checkpoint:** keep the work diagnostic-only. Do not change playable-world topology until the remaining invariants and the pure-hexagonal-prism compatibility question are resolved.
