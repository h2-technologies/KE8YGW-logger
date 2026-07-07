# Award Engine

The award engine computes rebuildable progress from QSO projections. It does not read or mutate official events directly during normal operation.

## Principles

- Deleted QSOs do not count by default.
- Restored QSOs count again after projection replay.
- Duplicate credits, such as repeated DXCC entities, count once.
- Confirmation state is modeled as projected QSO payload data, even though real confirmation integrations are future work.
- Award definitions are plugin-provided in the architecture; the MVP ships core definitions.

## MVP Definitions

- `dxcc.basic`: unique DXCC/entity credits.
- `was.basic`: unique US state credits.
- `pota.unique_parks`: placeholder for unique POTA park credits.
- `sota.unique_summits`: placeholder for unique SOTA summit credits.
- `grid.count`: placeholder for unique Maidenhead grid credits.

## GUI

The Awards workspace displays progress cards and credit previews. Rebuild commands recompute progress from the current QSO projection.

## Current Limitations

- No full award rule database.
- No confirmation download integration.
- No award submission/export workflow.
- Needed/missing lists are placeholders until rule databases are added.
