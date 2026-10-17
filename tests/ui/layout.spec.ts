import { test, expect } from "@playwright/test";

for (const viewport of [
  { width: 1600, height: 980 },
  { width: 1280, height: 810 },
  { width: 960, height: 610 },
]) {
  test(`fixed page and scrollable results at ${viewport.width}x${viewport.height}`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await page.goto("/");
    await expect(
      page.getByRole("heading", { name: "让灵感，有更多空间。" }),
    ).toBeVisible();
    const layout = await page.evaluate(() => {
      const workspace = document.querySelector(".workspace")!;
      const footer = document
        .querySelector(".selection-bar")!
        .getBoundingClientRect();
      const results = document.querySelector(".result-scroll")!;
      const resultBounds = results.getBoundingClientRect();
      return {
        bodyOverflow: getComputedStyle(document.body).overflow,
        workspaceOverflow: getComputedStyle(workspace).overflowY,
        documentOverflow: document.documentElement.scrollHeight > innerHeight,
        footerBottom: footer.bottom,
        footerTop: footer.top,
        listHeight: resultBounds.height,
        listBottom: resultBounds.bottom,
        listOverflow: getComputedStyle(results).overflowY,
      };
    });
    expect(layout.bodyOverflow).toBe("hidden");
    expect(layout.workspaceOverflow).toBe("hidden");
    expect(layout.documentOverflow).toBe(false);
    expect(layout.footerBottom).toBeLessThanOrEqual(viewport.height);
    expect(layout.listHeight).toBeGreaterThan(90);
    expect(layout.listBottom).toBeLessThanOrEqual(layout.footerTop);
    expect(layout.listOverflow).toBe("auto");
    // Exercise actual modal and keyboard behavior; no native file access is mocked.
    await page.getByRole("button", { name: "调整范围", exact: true }).click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).not.toBeVisible();
    if (viewport.width === 1280)
      await page.screenshot({ path: "docs/screenshots/desktop-fixed.png" });
    // Force overflowing list content without changing application or backend state.
    await page.locator(".result-scroll").evaluate((el) => {
      const content = document.createElement("div");
      content.style.height = "4000px";
      content.textContent = "Layout overflow fixture";
      el.replaceChildren(content);
    });
    await page.locator(".result-scroll").hover();
    await page.mouse.wheel(0, 500);
    await expect
      .poll(() => page.locator(".result-scroll").evaluate((el) => el.scrollTop))
      .toBeGreaterThan(0);
    expect(await page.evaluate(() => document.documentElement.scrollTop)).toBe(
      0,
    );
  });
}
