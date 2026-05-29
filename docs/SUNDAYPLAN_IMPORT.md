# SundayPlan → SundayStage import

SundayStage can import a service plan from **SundayPlan** (the suite's
volunteer-scheduling + planning app). SundayPlan owns _what to play and in what
order_; SundayStage turns that into the live presentation.

## Status

SundayPlan has **no export feature yet** — its data lives in Supabase and a
proper export is a later phase. Until then, this importer accepts a JSON file in
the interchange shape below (mirroring SundayPlan's `Service` + `Setlist`
model). When SundayPlan ships export, its JSON drops straight in with no changes
here.

## How matching works

- **Songs are matched by title** (case-insensitive) against the current
  library — SundayPlan's internal ids don't exist here.
- A title with **no local match** is created as an **empty stub song** so
  nothing is lost; you fill in the lyrics afterwards. The importer reports which
  titles it stubbed.
- **Scripture** is added as a labelled placeholder (the importer won't guess a
  Bible translation); wire it up in the Bible module. Reported as a warning.
- Other items (gap/announcement) are added as labelled gaps.

The importer returns: the new service, the count of matched songs, the list of
stubbed song titles, and any warnings.

## Interchange format

```json
{
  "name": "Sunday 14 June",
  "starts_at": 1718352000000,
  "notes": "Pinse — tema: Den hellige ånd",
  "items": [
    { "kind": "song", "title": "Amazing Grace", "key": "G" },
    { "kind": "song", "title": "Oceans" },
    { "kind": "scripture", "reference": "John 3:16" },
    { "kind": "gap", "label": "Kollekt" }
  ]
}
```

Field notes:

- `name` — service name (defaults to "Importert plan" if missing).
- `starts_at` — unix milliseconds. SundayPlan's `starts_at_utc` is also accepted.
- `notes` — optional planner notes, copied onto the service.
- `items[].kind` — `song` (default) | `scripture` | `gap` | `announcement`.
- `items[].title` — song title (the match key).
- `items[].key` / `key_override` — performance key for a song.
- `items[].reference` / `scripture_ref` — scripture reference.
- `items[].label` — label for a non-song item.

Unknown fields are ignored, and missing optional fields fall back to sensible
defaults — so a partial or future-extended plan still imports.

A ready-to-try sample lives at [`examples/sundayplan-example.json`](./examples/sundayplan-example.json).
