import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import { NavRail } from "./NavRail";

vi.mock("../server-api", () => ({
  fetchMe: vi.fn(async () => ({
    email: "dambi@example.com",
    user_id: "user_1",
  })),
  listFindings: vi.fn(async () => []),
  logout: vi.fn(),
}));

function renderNavRail() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <NavRail />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("NavRail", () => {
  it("removes focus from a pointer-clicked nav item so the rail can collapse on mouse leave", () => {
    renderNavRail();

    const simulationLink = screen.getByRole("link", { name: "Simulation" });
    simulationLink.focus();
    expect(document.activeElement).toBe(simulationLink);

    fireEvent.pointerUp(simulationLink);

    expect(document.activeElement).not.toBe(simulationLink);
  });
});
