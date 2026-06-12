// Regression: the module contract (liveBridge doc lines 20-22) promises a
// `now_playing` live event fires ONLY when the *song* under the cursor changes,
// so SundayRec recording chapters don't churn slide-by-slide within one song.
//
// This pins that contract at the pure-driver level.
import { describe, it, expect } from "vitest";

import {
  bridgeOnCueChange,
  type BridgeCue,
  type LiveBridgeContext,
} from "@/lib/liveBridge";
import { LiveSequence } from "@/lib/liveEmitter";

const CHURCH = "11111111-1111-1111-1111-111111111111";
const SERVICE = "33333333-3333-3333-3333-333333333333";

const ctx: LiveBridgeContext = {
  churchId: CHURCH,
  serviceId: SERVICE,
  serviceDate: "2026-05-31",
  wasStreamed: true,
  songsByItem: {
    "item-a": { songId: "song-a", title: "Amazing Grace", variantId: "arr-1" },
    "item-b": { songId: "song-b", title: "Oceans" },
  },
};

// Two cues of the SAME song item (Verse 1 → Chorus), then a different song.
const cues: BridgeCue[] = [
  {
    serviceItemId: "item-a",
    displayLabel: "Grace V1",
    sectionLabel: "Verse 1",
  },
  { serviceItemId: "item-a", displayLabel: "Grace C", sectionLabel: "Chorus" },
  {
    serviceItemId: "item-b",
    displayLabel: "Oceans V1",
    sectionLabel: "Verse 1",
  },
];

const types = (es: { type: string }[]) => es.map((e) => e.type);

describe("bridgeOnCueChange — now_playing only on song change", () => {
  it("does NOT emit now_playing when advancing within one song", () => {
    const seq = new LiveSequence();
    const shown = new Set<string>(["item-a"]); // already showing within this session
    const out = bridgeOnCueChange(ctx, cues, 0, 1, seq, 1_000, shown);
    expect(types(out.liveEvents)).toEqual(["cue.advanced"]);
  });

  it("DOES emit now_playing when the song under the cursor changes", () => {
    const seq = new LiveSequence();
    const shown = new Set<string>(["item-a"]);
    const out = bridgeOnCueChange(ctx, cues, 1, 2, seq, 1_000, shown);
    expect(types(out.liveEvents)).toEqual(["cue.advanced", "now_playing"]);
  });
});
