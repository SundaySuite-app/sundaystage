/**
 * Typed wrappers around Tauri's `invoke()`.
 *
 * One function per Rust command. Wraps `invoke<T>(name, args)` so:
 *   - The TypeScript caller has a stable signature
 *   - Rust `AppError` is rethrown as a JS `IPCError` the React code can
 *     catch
 *   - Dev-mode logs every call for debugging (toggle via `VITE_IPC_LOG`)
 *
 * Convention: command names are `entity_verb` (e.g. `song_list`,
 * `library_create`). Matches `commands::*` in Rust.
 */

import { invoke } from "@tauri-apps/api/core";

import type {
  AppError,
  Library,
  LibraryInput,
  SearchResult,
  Service,
  ServiceItem,
  Song,
  SongInput,
  SongSection,
} from "./bindings";

const DEV = import.meta.env.DEV;
const LOG_IPC = DEV && import.meta.env.VITE_IPC_LOG !== "false";

/** Wrapper around Tauri's error that preserves the Rust `code` field. */
export class IPCError extends Error {
  readonly code: AppError["code"];
  constructor(err: AppError) {
    super(err.message);
    this.code = err.code;
    this.name = "IPCError";
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (LOG_IPC) {
    console.debug(`[ipc] → ${cmd}`, args);
  }
  try {
    const out = await invoke<T>(cmd, args);
    if (LOG_IPC) console.debug(`[ipc] ← ${cmd}`, out);
    return out;
  } catch (raw) {
    // Tauri rethrows serialised AppError as plain object
    if (
      raw &&
      typeof raw === "object" &&
      "code" in raw &&
      "message" in raw
    ) {
      throw new IPCError(raw as AppError);
    }
    if (raw instanceof Error) throw raw;
    throw new Error(String(raw));
  }
}

// ── Library ──────────────────────────────────────────────────────────────────

export const library = {
  create: (input: LibraryInput) => call<Library>("library_create", { input }),
  get:    (id: string)          => call<Library>("library_get", { id }),
  list:   ()                     => call<Library[]>("library_list"),
  rename: (id: string, name: string) =>
    call<Library>("library_rename", { id, name }),
};

// ── Song ─────────────────────────────────────────────────────────────────────

export const song = {
  create: (input: SongInput) => call<Song>("song_create", { input }),
  get:    (id: string)       => call<Song>("song_get", { id }),
  list:   (libraryId: string, limit = 100, offset = 0) =>
    call<Song[]>("song_list", { libraryId, limit, offset }),
  delete: (id: string) => call<void>("song_delete", { id }),
  search: (libraryId: string, query: string, limit = 50) =>
    call<SearchResult[]>("song_search", { libraryId, query, limit }),
  sections: (songId: string) =>
    call<SongSection[]>("song_sections", { songId }),
  addSection: (songId: string, label: string, lyrics: string) =>
    call<SongSection>("song_add_section", { songId, label, lyrics }),
};

// ── Service ──────────────────────────────────────────────────────────────────

export const service = {
  create: (libraryId: string, name: string, startsAt: number) =>
    call<Service>("service_create", { libraryId, name, startsAt }),
  get: (id: string) => call<Service>("service_get", { id }),
  upcoming: (libraryId: string, from = 0, limit = 20) =>
    call<Service[]>("service_upcoming", { libraryId, from, limit }),
  items: (serviceId: string) =>
    call<ServiceItem[]>("service_items", { serviceId }),
};

/** Bundled namespace for ergonomic imports. */
export const ipc = { library, song, service };
