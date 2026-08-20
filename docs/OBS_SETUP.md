# OBS 1080p60 本地输出

Pocket Live 的 MVP 输出是一个最终合成完成的、不透明 1920×1080 窗口。OBS
只负责窗口采集与硬件编码，不需要绿幕滤镜，也不接触 Vision 或原始姿态数据。

## 1. 启动 Pocket Live

```sh
bun run live:build
bun run live
```

正式角色不要使用仓库的 VRoid 测试样例。先检查自有 VRM 0.x 是否具备骨骼和表情：

```sh
target/release/pocket-character --model-info /absolute/path/to/hero.vrm
bun run live -- --model /absolute/path/to/hero.vrm
```

报告里的 `compatible` 应为 `true`；至少要有头、颈、脊柱、双臂骨骼，以及眨眼和口型。

`bun run live` 的默认等价参数是：

```sh
target/release/pocket-character \
  --tracking camera \
  --output-size 1920x1080 \
  --background virtual \
  --max-fps 60
```

可选背景模式：

- `--background virtual`：推荐直播模式；只输出数字人和虚拟背景，摄像头仅在后台驱动动作。
- `--background split`：可选双屏模式；真人只在左屏，数字替身只在右屏，并用分割线硬隔离。
- `--background matte`：旧的同屏换皮模式；保留房间，将 person matte 覆盖的真人区域换成漫画背景。
- `--background clean --clean-plate-delay 5`：启动后五秒采集干净背景；倒计时内离开画面。
- `--background camera`：显示摄像头原画，用于排障。
- `--background transparent`：恢复原来的 450×600 透明桌面挂件模式。

如果有多个摄像头，先运行 `bun run vision:list`，再传
`--device CAMERA_UNIQUE_ID`。所有识别和合成都在本机完成。

## 2. 建立 OBS 场景

1. 在 OBS 的“视频”设置中把基础画布与输出分辨率都设为 `1920x1080`，帧率设为 `60`。
2. 添加 macOS“窗口采集”源，选择标题为 `pocket-character` 的窗口。
3. 让该源铺满画布；不要添加色键或人像抠像滤镜，因为 Pocket 已输出最终画面。
4. 声音直接由 OBS 的独立麦克风输入管理；Pocket Live 不包含变声功能。
5. 在直播输出中选择 Apple VideoToolbox 的 H.264 硬件编码器；关键帧间隔使用平台通常要求的 2 秒，码率按直播平台要求填写。

第一次窗口采集时，macOS 可能要求给 OBS“屏幕与系统音频录制”权限。授权后需要重启
OBS。直播期间保持 Pocket 窗口未最小化；可以放到独立桌面空间，但不要让系统暂停它。

## 3. 上播前检查

```sh
# 不打开摄像头：校验资产、离线面、Swift/Rust 协议和 1080p 合成成片
bun run live:diagnose

# 明确打开一个可用摄像头，只读取三帧，不落盘
bun run live:diagnose --camera

# 完整 1080p VRM + mock pose + person matte 的 GPU 吞吐基准
bun run live:benchmark
```

OBS 的统计窗口应保持渲染/编码丢帧为 0。若出现掉帧，依次降低直播编码码率、关闭
OBS 预览、把 Vision 跟踪率从默认 30 调低；不要降低 Pocket 的 60 Hz 渲染循环。

## 4. 当前边界

窗口采集是 MVP 的稳定输出契约。它会比直接共享 Metal texture 多一次系统合成路径。
后续生产版本可增加 IOSurface/Syphon OBS source，但不会改变 Vision、pose solver 或
Pocket compositor 的数据契约。
