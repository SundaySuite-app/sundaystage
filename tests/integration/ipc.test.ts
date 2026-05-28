// Unit smoke — the IPC error wrapper. The contract is that a Rust AppError
// (a {code, message} object) becomes a JS Error subclass that preserves the
// `code` field so React code can branch on it.
import { describe, it, expect } from "vitest";

import { IPCError } from "@/lib/ipc";

describe("IPCError", () => {
  it("preserves the Rust error code and message", () => {
    const err = new IPCError({ code: "NotFound", message: "song missing" });
    expect(err).toBeInstanceOf(Error);
    expect(err.name).toBe("IPCError");
    expect(err.code).toBe("NotFound");
    expect(err.message).toBe("song missing");
  });
});
