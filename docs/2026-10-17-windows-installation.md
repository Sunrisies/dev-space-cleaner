# 0.1.4：Windows 搜索与安装包

原先交付的是便携版 exe，只放在项目目录中，没有创建系统应用入口，因此系统或启动器的应用搜索不一定能发现。

应用显示名称统一为“缓境 Huanj”，中文和英文均出现在开始菜单名称中。应用标识继续使用 `dev.huanj.cleaner`，原有扫描设置路径不变。

新增 `npm run build:installer`，使用 Tauri NSIS 当前用户模式生成安装包，支持简体中文和英文。安装时创建固定安装目录、开始菜单入口及 Windows 卸载记录；不需要用户手工创建快捷方式。打包方式参考 [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/)。

本机执行与验证：

- 构建 `缓境 Huanj_0.1.4_x64-setup.exe` 成功，约 2.02 MiB；交付副本为 `releases/huanj-0.1.4-setup.exe`。
- 以当前用户安装，安装器退出码 0。
- 开始菜单入口名称为“缓境 Huanj”，目标为本机用户 LocalAppData 下 `缓境 Huanj/huanj.exe`，目标存在。
- `Get-StartApps` 返回名称“缓境 Huanj”和 AppID `dev.huanj.cleaner`。
- HKCU 卸载记录显示名称“缓境 Huanj”、版本 `0.1.4` 和固定安装位置。
- 安装前后比较已保存设置的文件字节，内容一致。

第三方启动器可能使用自己的应用索引。已验证 Windows 的应用目录注册，没有逐一操作或修改第三方启动器的索引设置。
