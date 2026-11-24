// Timing and lifecycle in the injected scripts, on Playwright's fake clock, with the page served
// under Meet's Trusted Types CSP (which every popup inherits).
const { test, expect } = require("@playwright/test");
const { openMeet, ipcCalls, model, TRUSTED_TYPES_CSP } = require("./harness");

async function openCall(page, { reject } = {}) {
  const errors = [];
  page.context().on("page", (p) => p.on("pageerror", (e) => errors.push(`${p.url()}: ${e}`)));
  page.on("pageerror", (e) => errors.push(`${page.url()}: ${e}`));
  await openMeet(page, "self-and-bot.html", { csp: TRUSTED_TYPES_CSP, clock: true, reject });
  await page.clock.pauseAt(Date.now() + 60_000);
  // Advance until your camera reads live (real video frames under the fake clock).
  await expect
    .poll(async () => {
      await page.clock.runFor(300);
      return (await model(page)).self.stream;
    })
    .toBe(true);
  if (!reject) {
    // The popups open once Rust holds the call: the next popup check after the report lands.
    await page.clock.runFor(250);
  }
  return errors;
}

const popups = (page) => page.evaluate(() => window.__cooeePopups.diag());

test("a meet_state the IPC rejects is sent again, unchanged, on the next tick, and only then", async ({ page }) => {
  await openCall(page);
  const before = (await ipcCalls(page, "meet_state")).length;

  await page.evaluate(() => {
    window.__fake.rejectNext("meet_state");
    document.querySelector('[aria-label="Leave call"]').remove();
  });
  await page.clock.runFor(300); // the mutation's refresh: reported, rejected
  await page.clock.runFor(1000); // the next tick: resent, accepted
  await page.clock.runFor(2000); // later ticks: nothing changed, nothing sent

  const sent = (await ipcCalls(page, "meet_state")).slice(before);
  expect(sent).toHaveLength(2);
  expect(sent[0]).toMatchObject({ inCall: false });
  expect(sent[1]).toEqual(sent[0]);
});

test("a camera frozen for under the grace stays on; frozen past it, it reads off", async ({ page }) => {
  await openCall(page);
  const self = async () => (await model(page)).participants.find((p) => p.isSelf);
  expect((await self()).cameraOn).toBe(true);

  // A refresh while the video is still live, then the freeze.
  await page.evaluate(() => document.body.classList.toggle("nudge"));
  await page.clock.runFor(250);
  await page.evaluate(() => {
    const v = [...document.querySelectorAll('[data-participant-id="spaces/fixture/devices/1"] video')][1];
    window.__fake.setMuted(v.srcObject.getVideoTracks()[0], true);
  });

  await page.clock.runFor(1000);
  expect((await self()).cameraOn).toBe(true);
  await page.clock.runFor(2000);
  expect((await self()).cameraOn).toBe(false);
});

test("the popups build under Meet's Trusted Types CSP", async ({ page }) => {
  const errors = await openCall(page);
  const diag = await popups(page);
  expect(diag.faces).toMatchObject({ popupOpen: true, doc: { built: true } });
  expect(diag.controls).toMatchObject({ popupOpen: true, doc: { built: true } });
  expect(errors).toEqual([]);
});

test("a popup destroyed while the call still wants it opens again", async ({ page }) => {
  await openCall(page);
  const faces = page.context().pages().find((p) => p.url().endsWith("#faces"));
  await faces.close();
  expect((await popups(page)).faces.popupOpen).toBe(false);

  await page.clock.runFor(250);
  expect((await popups(page)).faces).toMatchObject({ popupOpen: true, doc: { built: true } });
});

test("a popup whose document is replaced (Tauri's own about:blank load) is rebuilt", async ({ page }) => {
  await openCall(page);
  const faces = page.context().pages().find((p) => p.url().endsWith("#faces"));
  await faces.goto("about:blank");
  expect((await popups(page)).faces).toMatchObject({ popupOpen: true, doc: { built: false } });

  await page.clock.runFor(250);
  expect((await popups(page)).faces).toMatchObject({ popupOpen: true, doc: { built: true } });
});

// Meet re-renders without its Leave button, then puts it back.
const dropMarker = (page) =>
  page.evaluate(() => {
    const b = document.querySelector('[aria-label="Leave call"]');
    window.__marker = { b, parent: b.parentNode };
    b.remove();
  });
const restoreMarker = (page) => page.evaluate(() => window.__marker.parent.append(window.__marker.b));

test("the page keeps its popups through a re-render that drops the in-call marker", async ({ page }) => {
  const errors = await openCall(page);
  const faces = page.context().pages().find((p) => p.url().endsWith("#faces"));
  const windows = page.context().pages().length;

  await dropMarker(page);
  await page.clock.runFor(1000);
  expect((await popups(page)).faces).toMatchObject({ popupOpen: true, doc: { built: true } });

  await restoreMarker(page);
  await page.clock.runFor(1000);
  expect(faces.isClosed()).toBe(false);
  expect(page.context().pages()).toHaveLength(windows);
  expect((await popups(page)).faces).toMatchObject({ popupOpen: true, doc: { built: true } });
  expect(errors).toEqual([]);
});

test("a popup Rust destroys while the call is unwanted stays closed until the call wants it", async ({ page }) => {
  await openCall(page);
  await dropMarker(page);
  await page.clock.runFor(300);
  await page.context().pages().find((p) => p.url().endsWith("#faces")).close();

  await page.clock.runFor(1000);
  expect((await popups(page)).faces.popupOpen).toBe(false);

  await restoreMarker(page);
  await page.clock.runFor(1000);
  expect((await popups(page)).faces).toMatchObject({ popupOpen: true, doc: { built: true } });
});

test("no popup opens until Rust holds the call state that wants it", async ({ page }) => {
  const REJECTED = 5;
  await openCall(page, { reject: { meet_state: REJECTED } });
  expect((await ipcCalls(page, "meet_state")).length).toBeLessThanOrEqual(REJECTED);
  const diag = await popups(page);
  expect(diag.faces.popupOpen).toBe(false);
  expect(diag.controls.popupOpen).toBe(false);

  await page.clock.runFor(10_000);
  expect((await ipcCalls(page, "meet_state")).length).toBeGreaterThan(REJECTED);
  expect((await popups(page)).faces).toMatchObject({ popupOpen: true, doc: { built: true } });
  expect((await popups(page)).controls).toMatchObject({ popupOpen: true, doc: { built: true } });
});
