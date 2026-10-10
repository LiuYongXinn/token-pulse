import { expect, type Locator } from '@playwright/test';

export async function columnBounds(table: Locator) {
  return table.evaluate(element => [...element.querySelectorAll(':scope > thead > tr > th')].map(header => {
    const { x, width } = header.getBoundingClientRect();
    return { x: x - element.getBoundingClientRect().x, width };
  }));
}

export async function expectStableColumns(table: Locator, before: Awaited<ReturnType<typeof columnBounds>>) {
  const after = await columnBounds(table);
  expect(after).toHaveLength(before.length);
  after.forEach((column, index) => {
    expect(Math.abs(column.x - before[index].x)).toBeLessThan(1);
    expect(Math.abs(column.width - before[index].width)).toBeLessThan(1);
  });
}

export async function expectStableTextColumns(table: Locator) {
  const before = await columnBounds(table);
  await table.locator(':scope > tbody > tr').first().locator('td').first().evaluate(cell => {
    cell.textContent = '019a54fe-3072-7196-9bc0-fb9d79bd98aa'.repeat(8);
  });
  await expectStableColumns(table, before);
  await table.locator(':scope > tbody > tr').first().locator('td').first().evaluate(cell => {
    cell.textContent = '会话标题与项目模型名称刷新后应该在固定列内换行'.repeat(8);
  });
  await expectStableColumns(table, before);
}
