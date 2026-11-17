// 拖放：侧栏里调整订阅源、文件夹、邮箱的顺序，以及把列表里的条目拖到侧栏的视图上。
//
// 用浏览器原生的拖放。拖动过程中读不到 dataTransfer 里的数据，所以正在拖的东西另外记在这里。

import type { DragEvent } from "react";
import { create } from "zustand";

export type DragPayload =
  | { kind: "items"; ids: string[] }
  | { kind: "feed"; id: string; folder: string }
  | { kind: "folder"; name: string }
  | { kind: "account"; id: string };

const MIME = "application/x-postflow";

export const useDrag = create<{ payload: DragPayload | null }>()(() => ({ payload: null }));

export function startDrag(e: DragEvent, payload: DragPayload, label: string) {
  e.dataTransfer.setData(MIME, JSON.stringify(payload));
  // 有些浏览器没有纯文本数据就不开始拖动
  e.dataTransfer.setData("text/plain", label);
  e.dataTransfer.effectAllowed = "move";
  useDrag.setState({ payload });
}

export function endDrag() {
  useDrag.setState({ payload: null });
}

/** 正在拖的是不是我们自己的东西（而不是从外面拖进来的文件、文字）。 */
export function isOurs(e: DragEvent): boolean {
  return e.dataTransfer.types.includes(MIME) && useDrag.getState().payload !== null;
}

export type DropPosition = "before" | "after" | "inside";

/**
 * 指针在目标的上半还是下半；`inside` 为真时分成三等份，中间那段算放进去。
 * 一行只有 28px，上下各留三分之一，拖到文件夹前后才不至于总被当成放进去。
 */
export function dropPosition(e: DragEvent, inside = false): DropPosition {
  const rect = e.currentTarget.getBoundingClientRect();
  return positionAt((e.clientY - rect.top) / Math.max(1, rect.height), inside);
}

/** `y` 是指针在行里的相对高度，0 是上沿，1 是下沿。 */
export function positionAt(y: number, inside = false): DropPosition {
  if (inside) return y < 1 / 3 ? "before" : y > 2 / 3 ? "after" : "inside";
  return y < 0.5 ? "before" : "after";
}

/** 把 `moved` 挪到 `target` 前面或后面，返回新的顺序。 */
export function moveInList<T>(list: T[], moved: T, target: T, position: "before" | "after"): T[] {
  const rest = list.filter((x) => x !== moved);
  const at = rest.indexOf(target);
  if (at === -1) return [...rest, moved];
  rest.splice(position === "before" ? at : at + 1, 0, moved);
  return rest;
}
