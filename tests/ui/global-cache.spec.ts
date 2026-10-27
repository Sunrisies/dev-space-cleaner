import { test, expect } from "@playwright/test";

test("global caches are opt-in, persist separately and explain cleanup impact", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1080, height: 650 });
  await page.addInitScript(() => {
    const host = window as any;
    host.isTauri = true;
    const callbacks = new Map<number, (value: unknown) => void>();
    let next = 1;
    const caches = [
      {
        id: "npm-cache",
        tool: "npm",
        path: "C:\\Users\\demo\\npm-cache\\_cacache",
        category: "共享下载缓存",
        impact: "之后安装可能需要重新联网下载",
      },
      {
        id: "go-build",
        tool: "Go",
        path: "C:\\Users\\demo\\go-build",
        category: "构建缓存",
        impact: "之后构建需要重新编译",
      },
    ];
    const items = caches.map((cache, index) => ({
      id: index + 1,
      root: "C:\\Users\\demo",
      path: cache.path,
      project: cache.tool,
      kind: index ? "go" : "node",
      rule: `global:${cache.id}`,
      evidence: `${cache.category}；${cache.impact}`,
      bytes: (index + 1) * 1024,
      files: 1,
      complete: true,
      measured: true,
      skipped_links: 0,
      errors: 0,
      modified: null,
    }));
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
        if (command === "load_preferences")
          return JSON.parse(
            localStorage.getItem("global-test-settings") || "null",
          );
        if (command === "save_preferences") {
          localStorage.setItem(
            "global-test-settings",
            JSON.stringify(args.settings),
          );
          return;
        }
        if (command === "list_global_caches") return caches;
        if (command === "start_scan") {
          host.scanArgs = args;
          let index = 0;
          const emit = (event: unknown) =>
            callbacks.get(args.onEvent.id)!({
              index: index++,
              message: { session: args.session, event },
            });
          for (const item of items) emit({ type: "sized", data: item });
          emit({
            type: "finished",
            data: {
              progress: {
                directories: 2,
                files: 2,
                found: 2,
                bytes: 3072,
                errors: 0,
                skipped_links: 0,
              },
              cancelled: false,
              elapsed_ms: 20,
              roots: caches.map((c) => c.path),
            },
          });
          return;
        }
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
  await expect(
    page.getByRole("region", { name: "扫描控制" }).getByText(/^全盘扫描/),
  ).toBeVisible();
  await page.getByRole("button", { name: "调整范围" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: /^全局缓存/ }).click();
  await dialog
    .getByRole("checkbox", { name: "npm 共享下载缓存 npm-cache" })
    .check();
  await dialog.getByRole("checkbox", { name: "Go 构建缓存 go-build" }).check();
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  const saved = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("global-test-settings")!),
  );
  expect(saved.global_mode).toBe(true);
  expect(saved.global_ids).toEqual(["go-build", "npm-cache"]);
  await page.reload();
  await expect(
    page.getByRole("region", { name: "扫描控制" }).getByText(/^全局缓存/),
  ).toBeVisible();
  await page.getByRole("button", { name: "开始扫描", exact: true }).click();
  await expect(page.locator(".main-metric .metric-value")).toContainText(
    "3 KiB",
  );
  expect(
    await page.evaluate(() => (window as any).scanArgs.globalIds.sort()),
  ).toEqual(["go-build", "npm-cache"]);
  expect(await page.evaluate(() => (window as any).scanArgs.kinds)).toEqual([]);
  await page.locator('[data-candidate-id="1"]').getByRole("checkbox").check();
  await page.locator('[data-candidate-id="2"]').getByRole("checkbox").check();
  await page.getByRole("button", { name: "清理所选目录" }).click();
  await expect(dialog.getByText(/全局缓存可能被多个项目共享/)).toBeVisible();
  await expect(dialog.getByText(/之后构建需要重新编译/)).toBeVisible();
  await dialog.getByRole("button", { name: "确认移入回收站" }).click();
  expect((await page.evaluate(() => (window as any).cleaned)).sort()).toEqual([
    1, 2,
  ]);
});
