# Android Alpha 发布

每次完成 APK 打包后，把对应提交、版本标签和 APK 一起推送至 GitHub Release。当前只发布 Alpha，并保持 `prerelease=true`、`latest=false`。

## 版本与提交

`android/version.properties` 是 Android 版本的唯一来源。下一次发布同时递增 `versionName` 的 Alpha 序号和 `versionCode`，例如 `0.2.0-alpha.2` / `4`。提交实现、说明与版本文件后再打包；发布标签必须指向这一提交，不允许复用旧版本名替换附件。

## 签名与环境

Alpha 使用固定的本机签名密钥，存放在仓库外。当前密钥沿用此前 Android 测试包的签名，允许覆盖升级并保留主机、信任与设备身份；首次配置不能默默生成另一把密钥。默认位置是 `~/.config/kite/signing/android-alpha.keystore`，仅当前用户可读。

可用 `KITE_ALPHA_KEYSTORE`、`KITE_ALPHA_KEY_ALIAS`、`KITE_ALPHA_STORE_PASSWORD`、`KITE_ALPHA_KEY_PASSWORD` 指定已有密钥。当前 Alpha 密钥使用 `androiddebugkey` 别名与本机测试签名口令；这是 GitHub Alpha 签名，不作为应用商店生产签名。口令和 keystore 不进入仓库、发布附件或日志。

需要 Android SDK / NDK、JDK 17+、Rust Android target、cargo-ndk 和已认证的 `gh`。网络代理、`JAVA_HOME`、`ANDROID_HOME`、`CARGO_TARGET_DIR` 由本机环境提供，不写入项目的全局配置。

## 发布

准备本次发布说明文件，注明行为变化、已完成验证与验收边界，然后执行：

```bash
python3 android/scripts/release-alpha.py --notes-file /absolute/path/release-notes.md
```

该命令要求干净的源码工作树，从 `origin` 识别 GitHub 仓库，构建带固定签名的 Release APK 并运行 Android lint。随后核对 APK 内版本、非 debuggable 属性、签名和 ABI，输出 APK、`SHA256SUMS` 与 `release-manifest.json` 到忽略目录 `android/release-output/<tag>/`。

命令推送当前源码分支与不可变标签，创建带附件的草稿预发布，下载并核验附件，再将其发布为 Alpha。若中断，重新执行同一命令；已经上传的附件必须逐字节一致才会复用，不会覆盖已发布的版本。完成后核对返回的 Release 链接，而不以打包成功代替发布成功。

可先运行 `--check-only` 检查版本、源码状态、远端和签名配置，不进行打包或网络写入。
