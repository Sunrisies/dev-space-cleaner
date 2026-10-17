import { test, expect, type Page } from "@playwright/test";
import { summarize } from "../../src/summary";
import type { Candidate } from "../../src/types";
const fixtures: Candidate[] = [
  {
    id: 1,
    path: "D:\\demo\\rust-alpha\\target",
    root: "D:\\",
    project: "rust-alpha",
    kind: "rust",
    evidence: "父目录包含 Cargo.toml",
    bytes: 1024,
    files: 1,
    complete: true,
    measured: true,
    skipped_links: 0,
    errors: 0,
    modified: 1700000000,
  },
  {
    id: 2,
    path: "D:\\demo\\rust-beta\\target",
    root: "D:\\",
    project: "rust-beta",
    kind: "rust",
    evidence: "父目录包含 Cargo.toml",
    bytes: 2048,
    files: 2,
    complete: true,
    measured: true,
    skipped_links: 0,
    errors: 0,
    modified: 1700000000,
  },
  {
    id: 3,
    path: "D:\\demo\\web-demo\\node_modules",
    root: "D:\\",
    project: "web-demo",
    kind: "node",
    evidence: "父目录包含 package.json",
    bytes: 4096,
    files: 3,
    complete: true,
    measured: true,
    skipped_links: 0,
    errors: 0,
    modified: 1700000000,
  },
  {
    id: 4,
    path: "D:\\demo\\python-demo\\.venv",
    root: "D:\\",
    project: "python-demo",
    kind: "python",
    evidence: "包含 pyvenv.cfg",
    bytes: 8192,
    files: 4,
    complete: true,
    measured: true,
    skipped_links: 0,
    errors: 0,
    modified: 1700000000,
  },
];

async function mockDesktop(page: Page, pending = false, failClean = false) {
  await page.setViewportSize({ width: 1080, height: 650 });
  await page.addInitScript(
    ({ fixtures, pending, failClean }) => {
      // Mock only native IPC. The real Channel, scan reducer, virtual list and menus run unchanged.
      const host = window as any;
      host.isTauri = true;
      host.opened = [];
      let next = 1;
      const callbacks = new Map();
      host.__TAURI_INTERNALS__ = {
        transformCallback(fn: unknown) {
          const id = next++;
          callbacks.set(id, fn);
          return id;
        },
        unregisterCallback(id: number) {
          callbacks.delete(id);
        },
        async invoke(command: string, args: any) {
          if (command === "list_drives") return [{ path: "D:\\", label: "D:" }];
          if (command === "load_preferences") return null;
          if (command === "open_candidate_directory") {
            host.opened.push(args);
            return;
          }
          if (command === "clean_selected")
            return args.ids.map((id: number) => ({
              id,
              success: !failClean,
              message: failClean
                ? "Windows 报告回收目标存在共享冲突，相关文件可能被占用。请关闭使用该目录的程序后重试（0x80270028）。未改用永久删除。"
                : "已移入回收站",
            }));
          if (command === "start_scan") {
            let index = 0;
            const emit = (event: unknown) =>
              callbacks.get(args.onEvent.id)({
                index: index++,
                message: { session: args.session, event },
              });
            const finish = () => {
              if (pending)
                emit({
                  type: "sized",
                  data: { ...fixtures[1], complete: false, errors: 1 },
                });
              emit({
                type: "finished",
                data: {
                  cancelled: false,
                  elapsed_ms: 100,
                  roots: ["D:\\"],
                  progress: {
                    directories: 10,
                    files: 10,
                    found: 4,
                    bytes: 15360,
                    errors: pending ? 1 : 0,
                    skipped_links: 0,
                  },
                },
              });
            };
            host.finishScan = finish;
            for (const item of fixtures) {
              if (pending && item.id === 2)
                emit({
                  type: "found",
                  data: { ...item, bytes: 0, measured: false },
                });
              else emit({ type: "sized", data: item });
            }
            if (!pending) finish();
            return;
          }
          throw Error("Unexpected IPC: " + command);
        },
      };
    },
    { fixtures, pending, failClean },
  );
  await page.goto("/");
  await page.getByRole("button", { name: "开始扫描", exact: true }).click();
  await expect(page.locator('[data-candidate-id="1"]')).toBeVisible();
}

test("type totals match rows, ignore search filters, and right click opens the scanned ID", async ({
  page,
}) => {
  await mockDesktop(page);
  const rust = page.getByRole("button", { name: /^Rust：2 个剩余目录，3 KiB/ });
  await expect(rust).toBeVisible();
  await expect(
    page.getByRole("button", { name: /^Node.js：1 个剩余目录，4 KiB/ }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: /^Python：1 个剩余目录，8 KiB/ }),
  ).toBeVisible();
  await expect(page.locator('[data-candidate-id="1"] .row-size')).toContainText(
    "1 KiB",
  );
  await page
    .getByRole("textbox", { name: "搜索项目或路径" })
    .fill("rust-alpha");
  await expect(page.locator(".result-row")).toHaveCount(1);
  await expect(rust).toBeVisible();
  await page.getByRole("button", { name: "清空搜索" }).click();
  await page.locator('[data-candidate-id="1"]').click({ button: "right" });
  await expect(page.getByRole("menu")).toBeVisible();
  await page.screenshot({ path: "docs/screenshots/language-totals-menu.png" });
  await page.getByRole("menuitem", { name: "打开目录", exact: true }).click();
  expect(await page.evaluate(() => (window as any).opened)).toEqual([
    { session: expect.any(String), id: 1 },
  ]);
  await expect(page.getByRole("menu")).not.toBeVisible();
  await page.locator('[data-candidate-id="2"]').focus();
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menu")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).not.toBeVisible();
});

test("pending and incomplete sums are labelled and cleanup updates remaining type and overall totals", async ({
  page,
}) => {
  await mockDesktop(page, true);
  await expect(
    page.getByRole("button", { name: /^Rust：2 个剩余目录，1 KiB · 统计中/ }),
  ).toBeVisible();
  await expect(page.locator('[data-candidate-id="2"] .row-size')).toContainText(
    "计算中",
  );
  await page.evaluate(() => (window as any).finishScan());
  await expect(
    page.getByRole("button", { name: /^Rust：2 个剩余目录，3 KiB · 部分统计/ }),
  ).toBeVisible();
  await expect(page.locator('[data-candidate-id="2"] .row-size')).toContainText(
    "部分统计",
  );
  await page.locator('[data-candidate-id="1"]').getByRole("checkbox").check();
  await page.getByRole("button", { name: "清理所选目录" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "确认移入回收站" })
    .click();
  await expect(
    page.getByRole("button", { name: /^Rust：1 个剩余目录，2 KiB · 部分统计/ }),
  ).toBeVisible();
  await expect(page.locator(".main-metric .metric-value")).toContainText(
    "14 KiB",
  );
  await page.locator('[data-candidate-id="1"]').click({ button: "right" });
  await expect(
    page.getByRole("menuitem", { name: "打开目录", exact: true }),
  ).toBeDisabled();
});

test("failed cleanup lists its full path and actionable reason, and can reopen the folder", async ({
  page,
}) => {
  await mockDesktop(page, false, true);
  await page.locator('[data-candidate-id="1"]').getByRole("checkbox").check();
  await page.getByRole("button", { name: "清理所选目录" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "确认移入回收站" })
    .click();
  const report = page.getByRole("dialog");
  await expect(
    report.getByRole("heading", { name: "清理失败目录" }),
  ).toBeVisible();
  await expect(
    report.getByText(fixtures[0].path, { exact: true }),
  ).toBeVisible();
  await expect(report.getByText(/0x80270028/)).toContainText("共享冲突");
  await expect(
    page.getByRole("button", { name: /^Rust：2 个剩余目录，3 KiB · 待复核/ }),
  ).toBeVisible();
  await page.screenshot({
    path: "docs/screenshots/recycle-failure-report.png",
  });
  await report.getByRole("button", { name: "打开目录", exact: true }).click();
  expect(await page.evaluate(() => (window as any).opened)).toEqual([
    { session: expect.any(String), id: 1 },
  ]);
  await page.getByRole("button", { name: /^查看失败目录/ }).click();
  await expect(
    page.getByRole("dialog").getByText(fixtures[0].path, { exact: true }),
  ).toBeVisible();
});

test("aggregation preserves large values and distinguishes pending, partial and removed records", () => {
  const items = [
    { ...fixtures[0], bytes: 5 * 1024 ** 3 + 123 },
    { ...fixtures[1], bytes: 2 * 1024 ** 3 + 77, complete: false },
    { ...fixtures[2], measured: false, bytes: 0 },
  ];
  const all = summarize(items, new Set());
  expect(all.rust).toMatchObject({
    count: 2,
    bytes: 7 * 1024 ** 3 + 200,
    incomplete: 1,
    pending: 0,
  });
  expect(all.node.pending).toBe(1);
  expect(summarize(items, new Set([1])).rust).toMatchObject({
    count: 1,
    bytes: 2 * 1024 ** 3 + 77,
    removed: 1,
  });
});
