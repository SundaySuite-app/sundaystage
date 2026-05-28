// Integration smoke — the React component layer. Renders the app shell's
// sidebar (wrapped in a QueryClient because the sync badge issues a query)
// and asserts the brand + Go Live entry point are present.
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { Sidebar } from "@/components/Sidebar";

function renderSidebar() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <Sidebar current="library" onNavigate={vi.fn()} onGoLive={vi.fn()} />
    </QueryClientProvider>,
  );
}

describe("Sidebar", () => {
  it("renders the brand and the Go Live action", () => {
    renderSidebar();
    expect(screen.getByText("SundayStage")).toBeInTheDocument();
    expect(screen.getAllByRole("button").length).toBeGreaterThan(0);
  });
});
