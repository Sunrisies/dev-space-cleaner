import { test, expect } from "@playwright/test";

test("saved results remain historical until an individual directory is revalidated", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1080, height: 650 });
  await page.addInitScript(() => {
    const host = window as any;
    host.isTauri = true;
    const callbacks = new Map<number, (value: unknown) => void>();
    let next = 1;
    const item = {
      id: 1,
      path: "D:\\demo\\project\\target",
      root: "D:\\",
      project: "project",
      kind: "rust",
      rule: "cargo-project",
      evidence: "Cargo.toml",
      bytes: 1024,
      files: 1,
      complete: true,
      measured: true,
      skipped_links: 0,
      errors: 0,
      modified: 1700000000,
    };
    const settings = {
      version: 1,
      mode: "all",
      roots: [],
      kinds: ["rust"],
      workers: 8,
      global_mode: false,
      global_ids: [],
      history_enabled: true,
    };
    host.__TAURI_INTERNALS__ = {
      transformCallback(fn: (value: unknown) => void) {
        const id = next++;
        callbacks.set(id, fn);
        return id;
      },
      unregisterCallback(id: number) {
        callbacks.delete(id);
      },
      async invoke(command: string, args: any) {
        if (command === "list_drives") return [{ path: "D:\\", label: "D:" }];
        if (command === "load_preferences") return settings;
        if (command === "load_scan_history")
          return JSON.parse(localStorage.getItem("history-fixture") || "null");
        if (command === "save_scan_history") {
          localStorage.setItem(
            "history-fixture",
            JSON.stringify(args.snapshot),
          );
          return;
        }
        if (command === "start_scan") {
          let index = 0;
          const emit = (event: unknown) =>
            callbacks.get(args.onEvent.id)!({
              index: index++,
              message: { session: args.session, event },
            });
          emit({ type: "sized", data: item });
          emit({
            type: "finished",
            data: {
              progress: {
                directories: 2,
                files: 1,
                found: 1,
                bytes: 1024,
                errors: 0,
                skipped_links: 0,
              },
              cancelled: false,
              elapsed_ms: 10,
              roots: ["D:\\"],
            },
          });
          return;
        }
        if (command === "refresh_candidate") {
          if (localStorage.getItem("history-refresh-fail") === "1")
            throw Error("识别依据已变化");
          host.refreshed = args;
          return { ...item, bytes: 3072 };
        }
        if (command === "preview_selected")
          return args.ids.map((id: number) => ({
            id,
            valid: true,
            message: "识别依据与目录身份校验通过",
          }));
        if (command === "clean_selected") {
          host.cleaned = args.ids;
          return args.ids.map((id: number) => ({
            id,
            success: true,
            message: "已移入回收站",
          }));
        }
        throw Error(`Unexpected IPC: ${command}`);
      },
    };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "开始扫描", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("history-fixture")))
    .not.toBeNull();
  const original = await page.evaluate(() =>
    localStorage.getItem("history-fixture"),
  );
  await page.reload();
  await expect(page.locator(".main-metric .metric-value")).toContainText(
    "历史数据",
  );
  const row = page.locator('[data-candidate-id="1"]');
  await expect(row.getByRole("checkbox")).toBeDisabled();
  await row.click({ button: "right" });
  await expect(page.getByRole("menuitem", { name: "打开目录" })).toBeDisabled();
  await page.getByRole("menuitem", { name: "查看详情" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText(/需要重新核实/)).toBeVisible();
  await page.evaluate(() => localStorage.setItem("history-refresh-fail", "1"));
  await dialog.getByRole("button", { name: "刷新此目录" }).click();
  await expect(dialog.getByRole("alert")).toContainText("识别依据已变化");
  await page.evaluate(() => localStorage.removeItem("history-refresh-fail"));
  await dialog.getByRole("button", { name: "刷新此目录" }).click();
  await expect(dialog.getByText("3 KiB", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(row.getByRole("checkbox")).toBeEnabled();
  expect(await page.evaluate(() => (window as any).refreshed.rule)).toBe(
    "cargo-project",
  );
  expect(
    await page.evaluate(() => localStorage.getItem("history-fixture")),
  ).toBe(original);
  await row.getByRole("checkbox").check();
  await page.getByRole("button", { name: "清理所选目录" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "确认移入回收站" })
    .click();
  expect(await page.evaluate(() => (window as any).cleaned)).toEqual([1]);
});
