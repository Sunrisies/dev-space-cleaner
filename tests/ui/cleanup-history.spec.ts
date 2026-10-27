import { test, expect } from "@playwright/test";

test("cleanup history shows outcomes, searches paths and opens the system recycle bin", async ({
  page,
}) => {
  await page.setViewportSize({ width: 960, height: 570 });
  await page.addInitScript(() => {
    const host = window as any;
    host.isTauri = true;
    let next = 1;
    host.__TAURI_INTERNALS__ = {
      transformCallback() {
        return next++;
      },
      unregisterCallback() {},
      async invoke(command: string) {
        if (command === "load_preferences") return null;
        if (command === "list_drives") return [];
        if (command === "list_cleanup_history")
          return [
            {
              timestamp_ms: 1700000000000,
              path: "D:\\demo\\web\\node_modules",
              bytes: 4096,
              kind: "node",
              success: true,
              message: "已移入回收站",
            },
            {
              timestamp_ms: 1700000001000,
              path: "D:\\demo\\rust\\target",
              bytes: 8192,
              kind: "rust",
              success: false,
              message: "目录正在使用",
            },
          ];
        if (command === "open_recycle_bin") {
          host.openedRecycleBin = true;
          return;
        }
        throw Error(`Unexpected IPC: ${command}`);
      },
    };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "清理记录", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("已移入回收站").first()).toBeVisible();
  await expect(dialog.getByText("目录正在使用")).toBeVisible();
  await expect(dialog.getByText("8 KiB", { exact: false })).toBeVisible();
  await dialog.getByRole("textbox", { name: "搜索清理记录" }).fill("rust");
  await expect(dialog.getByText("目录正在使用")).toBeVisible();
  await expect(dialog.getByText(/node_modules/)).toHaveCount(0);
  await dialog.getByRole("button", { name: "打开系统回收站" }).click();
  expect(await page.evaluate(() => (window as any).openedRecycleBin)).toBe(
    true,
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollHeight > innerHeight,
    ),
  ).toBe(false);
});
