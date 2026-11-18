// meet-adapter.js against fixture Meet pages: what the model and the meet_state report say.
const { test, expect } = require("@playwright/test");
const { openMeet, ipcCalls, model } = require("./harness");

test("your tile is you with your camera on, named from text not an icon glyph; a notetaker is a bot", async ({ page }) => {
  await openMeet(page, "self-and-bot.html");
  await expect
    .poll(async () => (await model(page)).participants.find((p) => p.isSelf)?.cameraOn, { timeout: 5000 })
    .toBe(true);

  const { participants } = await model(page);
  expect(participants).toHaveLength(2);
  const self = participants.find((p) => p.isSelf);
  expect(self).toMatchObject({ name: "Alex Example", cameraOn: true, stream: true, isBot: false });
  const bot = participants.find((p) => !p.isSelf);
  expect(bot).toMatchObject({ name: "Fireflies.ai Notetaker", isBot: true });

  const reports = await ipcCalls(page, "meet_state");
  expect(reports.at(-1)).toMatchObject({ inCall: true, others: 0, adapterOk: true, capturing: true });
});

test("your own screen share, requested through the prototype, is never the share window's presentation", async ({ page }) => {
  await openMeet(page, "own-share.html");
  await expect
    .poll(async () => (await model(page)).participants.filter((p) => p.cameraOn).length, { timeout: 5000 })
    .toBe(3);

  expect((await model(page)).presentation).toBeNull();
  expect((await ipcCalls(page, "meet_state")).at(-1)).toMatchObject({ inCall: true, others: 1, sharing: false });
  // The picker settled, so darkfield is told it closed.
  expect(await ipcCalls(page, "share_picker_closed")).toHaveLength(1);
});
