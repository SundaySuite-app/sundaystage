// Integration smoke — the React component layer. Renders the converged
// operator console's transport bar (wrapped in a QueryClient because the sync
// badge + output controls issue queries) and asserts the Go Live entry point
// is present. Replaces the old Sidebar smoke test after the workspace redesign.
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { TransportBar } from "@/features/workspace/TransportBar";

function renderTransport() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const noop = vi.fn();
  return render(
    <QueryClientProvider client={qc}>
      <TransportBar
        services={[]}
        service={null}
        onSelectService={noop}
        onNewService={noop}
        onOpenBrowser={noop}
        isLive={false}
        canGoLive
        outputState={null}
        onGoLive={noop}
        onStop={noop}
        onBlackout={noop}
        onLogo={noop}
        onJump={noop}
        onStage={noop}
        onExport={noop}
        onSettings={noop}
      />
    </QueryClientProvider>,
  );
}

describe("TransportBar", () => {
  it("renders the Go Live action and is interactive", () => {
    renderTransport();
    expect(screen.getByText("Gå live")).toBeInTheDocument();
    expect(screen.getAllByRole("button").length).toBeGreaterThan(0);
  });
});
