import assert from "node:assert/strict";
import { test } from "node:test";
import { fitCardRows } from "./paging.ts";

const filled = (available: number, gap: number, fit: { rows: number; height: number }) =>
  fit.rows * (fit.height + gap) <= available && available - fit.rows * (fit.height + gap) < fit.rows;

test("cards stretch or shrink a little so whole rows fill the page", () => {
  // 截图里的情况：卡片 272 高、空隙 10，一页 750 高——原来放 2 行多一截，现在 3 行正好填满（缩小一点）
  const fit = fitCardRows(750, 272, 10, 120);
  assert.equal(fit.rows, 3);
  assert.ok(fit.height < 272 && fit.height > 272 * 0.7, String(fit.height));
  assert.ok(filled(750, 10, fit));
  // 放 2.3 行的高度：放 2 行，放大一点
  const grow = fitCardRows(650, 272, 10, 120);
  assert.equal(grow.rows, 2);
  assert.ok(grow.height > 272);
  assert.ok(filled(650, 10, grow));
});

test("never squeezes cards too much, and never leaves a cut-off row", () => {
  // 1.5 行：放 2 行，缩到原来的七成多
  assert.deepEqual(fitCardRows(423, 272, 10, 120), { rows: 2, height: 201 });
  // 1.4 行：放 1 行，放大（最多到 1.6 倍）
  assert.deepEqual(fitCardRows(400, 272, 10, 120), { rows: 1, height: 390 });
  // 缩到最矮也放不下第二行时，少放一行
  assert.deepEqual(fitCardRows(250, 120, 10, 120), { rows: 1, height: 192 });
  for (let available = 150; available < 2000; available += 7) {
    const f = fitCardRows(available, 272, 10, 120);
    assert.ok(f.rows >= 1 && f.height >= 120, `${available}: ${JSON.stringify(f)}`);
    assert.ok(f.rows * (f.height + 10) <= available || f.rows === 1, `${available}: ${JSON.stringify(f)}`);
  }
});

test("a window too short for one card shrinks it to fit, but not below the minimum", () => {
  assert.deepEqual(fitCardRows(200, 272, 10, 120), { rows: 1, height: 190 });
  assert.deepEqual(fitCardRows(100, 272, 10, 120), { rows: 1, height: 120 });
  assert.deepEqual(fitCardRows(0, 272, 10, 120), { rows: 1, height: 272 });
});
