import { test, expect } from "@playwright/test";

const key = "huanj-scan-settings-v1";
const preset = {
  version: 1,
  mode: "custom",
  roots: ["D:\\projects\\saved-app", "E:\\offline-project"],
  kinds: ["node", "python"],
  workers: 2,
};

test("restores custom roots, types and workers, then preserves them across save and reload", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(
    ({ key, preset }) => localStorage.setItem(key, JSON.stringify(preset)),
    { key, preset },
  );
  await page.reload();
  await expect(
    page.getByRole("region", { name: "扫描控制" }).getByText(/^自定义目录/),
  ).toBeVisible();
  await expect(
    page.getByText("2 个目录 · 已保存", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: /^Android：0 个剩余目录，未扫描/ }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "开始扫描", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "调整范围", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByRole("checkbox", { name: "Rust", exact: true }),
  ).not.toBeChecked();
  await expect(
    dialog.getByRole("checkbox", { name: "Android", exact: true }),
  ).not.toBeChecked();
  await expect(
    dialog.getByRole("checkbox", { name: "Python", exact: true }),
  ).toBeChecked();
  await expect(
    dialog.getByText(preset.roots[0], { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText(preset.roots[1], { exact: true }),
  ).toBeVisible();
  await expect(dialog.getByRole("combobox")).toHaveValue("2");
  await page.screenshot({ path: "docs/screenshots/settings-saved.png" });
  await dialog.getByRole("button", { name: /^所有磁盘/ }).click();
  await dialog.getByRole("combobox").selectOption("4");
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("region", { name: "扫描控制" }).getByText(/^全盘扫描/),
  ).toBeVisible();
  await expect(
    page
      .getByRole("region", { name: "扫描控制" })
      .getByText("已保存", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "调整范围", exact: true }).click();
  await expect(dialog.getByRole("combobox")).toHaveValue("4");
  await dialog.getByRole("button", { name: /^自定义目录/ }).click();
  await expect(
    dialog.getByText(preset.roots[0], { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText(preset.roots[1], { exact: true }),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  await page.reload();
  await expect(
    page.getByText("2 个目录 · 已保存", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("准备就绪", { exact: true })).toBeVisible();
});

test("reports corrupt saved settings without overwriting them on startup", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate((key) => localStorage.setItem(key, "{corrupt"), key);
  await page.reload();
  await expect(
    page.getByRole("status").filter({ hasText: "读取扫描设置失败" }),
  ).toBeVisible();
  expect(await page.evaluate((key) => localStorage.getItem(key), key)).toBe(
    "{corrupt",
  );
});

test("failed save keeps the previous stored settings and displays an error", async ({
  page,
}) => {
  await page.addInitScript(
    ({ key, preset }) => {
      localStorage.setItem(key, JSON.stringify(preset));
      const original = Storage.prototype.setItem;
      Storage.prototype.setItem = function (k, v) {
        if (k === key)
          throw new DOMException("Unavailable", "QuotaExceededError");
        return original.call(this, k, v);
      };
    },
    { key, preset },
  );
  await page.goto("/");
  await page.getByRole("button", { name: "调整范围", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox").selectOption("8");
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("无法保存扫描设置");
  expect(
    await page.evaluate((key) => JSON.parse(localStorage.getItem(key)!), key),
  ).toEqual(preset);
  await expect(
    dialog.getByRole("button", { name: "保存设置", exact: true }),
  ).toBeEnabled();
});

test("searchable project types save expanded selections and stay within a small window", async ({
  page,
}) => {
  await page.setViewportSize({ width: 960, height: 570 });
  await page.goto("/");
  await page.getByRole("button", { name: "选择扫描类型" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("checkbox")).toHaveCount(16);
  const search = dialog.getByRole("textbox", { name: "搜索语言或工具链" });
  await search.fill("Android");
  await expect(dialog.getByRole("checkbox")).toHaveCount(1);
  await dialog
    .getByRole("checkbox", { name: "Android", exact: true })
    .uncheck();
  await search.fill("Maven");
  await expect(
    dialog.getByRole("checkbox", { name: "Java / Kotlin", exact: true }),
  ).toBeChecked();
  await search.fill("不存在的语言");
  await expect(dialog.getByText("没有匹配的项目类型。")).toBeVisible();
  await search.fill("");
  await dialog
    .getByRole("checkbox", { name: "Zig", exact: true })
    .scrollIntoViewIfNeeded();
  await expect(
    dialog.getByRole("checkbox", { name: "Zig", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollHeight > innerHeight,
    ),
  ).toBe(false);
  await page.screenshot({ path: "docs/screenshots/toolchain-settings.png" });
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  const stored = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key)!),
    key,
  );
  expect(stored.kinds).toHaveLength(15);
  expect(stored.kinds).not.toContain("android");
  expect(stored.kinds).toContain("java");
  await page.reload();
  await page.getByRole("button", { name: "选择扫描类型" }).click();
  await expect(
    dialog.getByRole("checkbox", { name: "Android", exact: true }),
  ).not.toBeChecked();
  await expect(
    dialog.getByRole("checkbox", { name: "Java / Kotlin", exact: true }),
  ).toBeChecked();
});
