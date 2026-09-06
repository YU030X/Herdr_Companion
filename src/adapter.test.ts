import { afterEach, expect, it, vi } from "vitest";
import { subscribeToRuntime } from "./adapter";

const { listen } = vi.hoisted(() => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
afterEach(() => vi.resetAllMocks());

it("releases both event listeners on unsubscribe", async () => {
  const snapshotStop = vi.fn();
  const connectionStop = vi.fn();
  listen.mockResolvedValueOnce(snapshotStop).mockResolvedValueOnce(connectionStop);
  const stop = await subscribeToRuntime(vi.fn(), vi.fn());
  stop();
  expect(snapshotStop).toHaveBeenCalledOnce();
  expect(connectionStop).toHaveBeenCalledOnce();
});

it.each([0, 1])("cleans up partial registration when listener %i fails", async (failedIndex) => {
  const stop = vi.fn();
  const failure = new Error("registration failed");
  let finish!: (stop: () => void) => void;
  for (let index = 0; index < 2; index += 1) {
    if (index === failedIndex) listen.mockRejectedValueOnce(failure);
    else listen.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  }
  const result = subscribeToRuntime(vi.fn(), vi.fn());
  const rejection = expect(result).rejects.toBe(failure);
  finish(stop);
  await rejection;
  expect(stop).toHaveBeenCalledOnce();
});
