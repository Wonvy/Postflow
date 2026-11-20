// 分页视图里卡片的大小：一页正好放下整数行，不出现最后一行被遮住一截。只有纯函数，方便测试：npm run test:ui

/**
 * 一页放几行卡片、每行卡片多高，正好把 `available`（一页能放卡片的高度）填满。`natural` 是卡片原来的高度，
 * `gap` 是行与行之间的空隙，`min` 是卡片最矮能缩到多少（图片太小就看不清了）。
 *
 * 先按原来的大小算放得下几行（四舍五入），把卡片放大或缩小一点凑满；缩得太多（比原来矮三成以上）就少放一行、
 * 改为放大。放大最多到原来的 1.6 倍，再大就留点空，不把卡片撑得太夸张。窗口矮得一行都放不下时缩到正好放下一行。
 */
export function fitCardRows(
  available: number,
  natural: number,
  gap: number,
  min: number,
): { rows: number; height: number } {
  if (!(available > 0) || !(natural > 0)) return { rows: 1, height: natural };
  /** 放 `n` 行时每行多高正好填满。 */
  const fill = (n: number) => Math.floor(available / n) - gap;
  let rows = Math.max(1, Math.round(available / (natural + gap)));
  let height = fill(rows);
  if (height < Math.max(min, natural * 0.7) && rows > 1) {
    rows -= 1;
    height = fill(rows);
  }
  if (height < min) return { rows: 1, height: Math.max(min, fill(1)) };
  return { rows, height: Math.min(height, Math.round(natural * 1.6)) };
}
