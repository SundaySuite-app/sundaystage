// The printed cheat sheet (`?`) against the table it is generated from.
//
// `consoleKeys.test.ts` proves each row of `CONSOLE_SHORTCUTS` really resolves
// to the action it claims. This proves the modal actually renders that table
// rather than a second copy of it: add a key to the console and the sheet grows
// on its own; stop rendering the table and this fails. The hand-typed list this
// replaces could — and did — disagree with the console it described.
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";

import { ShortcutsModal } from "@/features/workspace/ShortcutsModal";
import { CONSOLE_SHORTCUTS, keyCap } from "@/features/workspace/consoleKeys";
import { translate } from "@/lib/i18n";

afterEach(cleanup);

const noop = () => {};

describe("the cheat sheet is the key table", () => {
  it("prints every group, row and keycap the console defines", () => {
    render(<ShortcutsModal onClose={noop} />);
    for (const group of CONSOLE_SHORTCUTS) {
      expect(
        screen.getAllByText(translate("no", group.heading)).length,
      ).toBeGreaterThan(0);
      for (const row of group.rows) {
        expect(
          screen.getAllByText(translate("no", row.label)).length,
          row.label,
        ).toBeGreaterThan(0);
        for (const stroke of row.strokes) {
          expect(
            screen.getAllByText(keyCap(stroke)).length,
            `${row.label}: ${keyCap(stroke)}`,
          ).toBeGreaterThan(0);
        }
      }
    }
  });

  it("teaches the section sequence a volunteer would otherwise never find", () => {
    render(<ShortcutsModal onClose={noop} />);
    expect(screen.getByText("Seksjonshopp")).toBeInTheDocument();
    expect(
      screen.getByText("Hopp til en seksjon — V2 = vers 2, R = refreng"),
    ).toBeInTheDocument();
  });

  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(<ShortcutsModal onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
