// 托盘菜单里要界面去做的事：写邮件、检查新内容、打开设置、开关靠边隐藏。
// 桌面端点了菜单项后先把窗口拿出来，再发事件过来（见 src-tauri/src/tray.rs）。

import { useEffect, useRef } from "react";
import { useActions } from "./actions";
import { api } from "./api";
import { useUi } from "./store";
import { isTauri } from "./transport";

const ACTION_EVENT = "shibapost://tray-action";

export function useTrayActions() {
  const actions = useActions();
  // 监听只注册一次，用最新的 actions
  const latest = useRef(actions);
  latest.current = actions;

  useEffect(() => {
    if (!isTauri) return;
    let stop: (() => void) | undefined;
    let alive = true;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<string>(ACTION_EVENT, ({ payload }) => {
        const ui = useUi.getState();
        switch (payload) {
          case "compose":
            latest.current.compose("new");
            break;
          case "refresh":
            // 邮件和订阅都查一遍
            void api.refresh();
            break;
          case "settings":
            ui.setDialog({ type: "settings" });
            break;
          case "edge":
            ui.setEdgeHide(!ui.edgeHide);
            break;
        }
      }).then((unlisten) => {
        if (alive) stop = unlisten;
        else unlisten();
      }),
    );
    return () => {
      alive = false;
      stop?.();
    };
  }, []);
}
