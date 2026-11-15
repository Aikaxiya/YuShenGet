# 提瓦特祈愿

原神祈愿记录的本地查询与统计分析工具。桌面客户端，所有数据只保存在本机。

## 界面示例

<table><tr>
<td width="50%"><img width="100%" alt="界面示例 1" src="https://github.com/user-attachments/assets/e802a72a-9166-4182-bd27-412ee0fb18f1" /></td>
<td width="50%"><img width="100%" alt="界面示例 2" src="https://github.com/user-attachments/assets/cbc79484-0c60-435e-bbec-a752fe63abeb" /></td>
</tr></table>

## 功能

- 分池统计：角色活动、武器活动、常驻、集录祈愿
- 保底、歪率、连歪、捕获明光、欧皇度评级
- 出金历史：列表 / 卡片视图，带角色头像与元素配色
- 增量同步：命中本地已有记录即停止翻页
- UIGF 导出

## 使用

1. 安装 `提瓦特祈愿_0.1.0_x64-setup.exe`。
2. 取祈愿记录链接：手机 HoYoGet（APK）导出后传到电脑，点「粘贴」；或在电脑上打开一次游戏「祈愿 → 祈愿记录」，点「从本地缓存读取」。
3. 点「点击查询」，完成后进入分析页。

authkey 是 24 小时有效的只读凭据。记录只写入本机，不上传任何服务器。

## 开发

- 技术栈：Tauri 2 + Vue 3 + Vite + Rust workspace
- 裸 exe：`npx tauri build --no-bundle`
- exe 安装包：`npx tauri build --bundles nsis`
- 数据目录：exe 同目录的 `data`
