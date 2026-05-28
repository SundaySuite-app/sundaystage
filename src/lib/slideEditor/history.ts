/**
 * Editor undo/redo — Phase 3.1.
 *
 * A command-pattern history (the build plan explicitly asks for this over a
 * generic state-diff). Each `Command` is an *absolute, block-scoped*
 * operation that knows how to apply itself forward and invert itself back.
 * Because commands replace specific blocks (rather than store deltas), they
 * compose cleanly with live drag previews: the canvas can call `preview()`
 * many times per second with no history churn, then `apply()` exactly one
 * command on pointer-up.
 */

import { useCallback, useMemo, useReducer } from "react";

import type { SlideBackground, SlideBlock, SlideDoc } from "@/lib/bindings";
import { addBlock, removeBlocks, replaceBlock, setBackground } from "./doc";

export interface Command {
  label: string;
  apply(doc: SlideDoc): SlideDoc;
  invert(doc: SlideDoc): SlideDoc;
}

// ── Command factories ────────────────────────────────────────────────────────

export function addBlockCommand(block: SlideBlock): Command {
  return {
    label: "Legg til element",
    apply: (doc) => addBlock(doc, block),
    invert: (doc) => removeBlocks(doc, [block.id]),
  };
}

export function updateBlockCommand(
  before: SlideBlock,
  after: SlideBlock,
): Command {
  return {
    label: "Endre element",
    apply: (doc) => replaceBlock(doc, after),
    invert: (doc) => replaceBlock(doc, before),
  };
}

export function setBackgroundCommand(
  before: SlideBackground,
  after: SlideBackground,
): Command {
  return {
    label: "Endre bakgrunn",
    apply: (doc) => setBackground(doc, after),
    invert: (doc) => setBackground(doc, before),
  };
}

/**
 * Replace the whole document in one step. Used for explicit, coarse actions
 * like "apply template" or "apply theme" that restyle/relayout an entire
 * slide — not for incremental edits (those stay block-scoped).
 */
export function replaceDocCommand(before: SlideDoc, after: SlideDoc): Command {
  return {
    label: "Bruk mal/tema",
    apply: () => after,
    invert: () => before,
  };
}

/** Bundle several commands into one undo step (e.g. moving a multi-selection). */
export function compositeCommand(label: string, cmds: Command[]): Command {
  return {
    label,
    apply: (doc) => cmds.reduce((d, c) => c.apply(d), doc),
    // Invert in reverse so each command undoes against the state it produced.
    invert: (doc) => [...cmds].reverse().reduce((d, c) => c.invert(d), doc),
  };
}

/**
 * Remove blocks, remembering each one's original index so undo restores both
 * the blocks and their stacking order.
 */
export function removeBlocksCommand(doc: SlideDoc, ids: string[]): Command {
  const removed = doc.blocks
    .map((block, index) => ({ block, index }))
    .filter(({ block }) => ids.includes(block.id));
  return {
    label: "Slett element",
    apply: (d) => removeBlocks(d, ids),
    invert: (d) => {
      const blocks = [...d.blocks];
      // Ascending index so earlier insertions don't shift later ones.
      for (const { block, index } of removed) {
        blocks.splice(Math.min(index, blocks.length), 0, block);
      }
      return { ...d, blocks };
    },
  };
}

// ── Hook ─────────────────────────────────────────────────────────────────────

interface HistoryState {
  doc: SlideDoc;
  undo: Command[];
  redo: Command[];
}

type Action =
  | { type: "apply"; cmd: Command }
  | { type: "preview"; doc: SlideDoc }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "reset"; doc: SlideDoc };

const MAX_DEPTH = 200;

function reducer(state: HistoryState, action: Action): HistoryState {
  switch (action.type) {
    case "apply": {
      const doc = action.cmd.apply(state.doc);
      const undo = [...state.undo, action.cmd].slice(-MAX_DEPTH);
      return { doc, undo, redo: [] };
    }
    case "preview":
      return { ...state, doc: action.doc };
    case "undo": {
      const cmd = state.undo[state.undo.length - 1];
      if (!cmd) return state;
      return {
        doc: cmd.invert(state.doc),
        undo: state.undo.slice(0, -1),
        redo: [...state.redo, cmd],
      };
    }
    case "redo": {
      const cmd = state.redo[state.redo.length - 1];
      if (!cmd) return state;
      return {
        doc: cmd.apply(state.doc),
        undo: [...state.undo, cmd],
        redo: state.redo.slice(0, -1),
      };
    }
    case "reset":
      return { doc: action.doc, undo: [], redo: [] };
  }
}

export interface EditorHistory {
  doc: SlideDoc;
  canUndo: boolean;
  canRedo: boolean;
  /** Run a command and push it onto the undo stack (clears redo). */
  apply: (cmd: Command) => void;
  /** Set the doc without touching history — for live drag/resize previews. */
  preview: (doc: SlideDoc) => void;
  undo: () => void;
  redo: () => void;
  /** Load a different slide; clears history. */
  reset: (doc: SlideDoc) => void;
}

export function useEditorHistory(initial: SlideDoc): EditorHistory {
  const [state, dispatch] = useReducer(reducer, {
    doc: initial,
    undo: [],
    redo: [],
  });

  const apply = useCallback(
    (cmd: Command) => dispatch({ type: "apply", cmd }),
    [],
  );
  const preview = useCallback(
    (doc: SlideDoc) => dispatch({ type: "preview", doc }),
    [],
  );
  const undo = useCallback(() => dispatch({ type: "undo" }), []);
  const redo = useCallback(() => dispatch({ type: "redo" }), []);
  const reset = useCallback(
    (doc: SlideDoc) => dispatch({ type: "reset", doc }),
    [],
  );

  return useMemo(
    () => ({
      doc: state.doc,
      canUndo: state.undo.length > 0,
      canRedo: state.redo.length > 0,
      apply,
      preview,
      undo,
      redo,
      reset,
    }),
    [
      state.doc,
      state.undo.length,
      state.redo.length,
      apply,
      preview,
      undo,
      redo,
      reset,
    ],
  );
}
