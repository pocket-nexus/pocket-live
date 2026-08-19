# Pocket Live：全本地实时 VRM 直播技术方案

状态：MVP 已实现并通过基础验收；生产长稳验收待最终设备组合执行
目标平台：Apple Silicon macOS 14+，首要验证设备为 Apple M5 Max
基线：`pocket-stack/pocket-character` + 固定版本的 PocketJS 子模块
硬约束：运行时不访问网络，不调用云服务，不依赖外部或本地 LLM

当前实现状态：Pocket 基线、Swift AVFoundation bridge、版本化 TrackingFrame
v3、共享内存中的 BGRA/person matte、本地 MediaPipe Face/Pose/Hand Tasks、
latest-value host client、逐骨链 One-Euro/速度异常门控、面部中性校准、脸/
躯干/左右臂独立丢失状态机、蛛丝手势门控、半身/头部/眼睛/表情 VRM 驱动和
最终 1080p Metal compositor 已落地。Apple Vision 只负责人像分割，不再与
MediaPipe 重复执行身体、手部或五官推理。MVP 通过 OBS 窗口采集输出；
IOSurface OBS source 保留为后续生产优化。

## 1. 交付目标

交付一个可复现、可断网运行的半身实时数字替身 MVP：摄像头中的单人动作驱动 VRM 角色，真人区域可被本地人像分割替换，Pocket 完成角色和最终场景渲染，OBS 获取 1080p60 画面并使用 Apple VideoToolbox 编码。

首版支持：

- 单个 1080p60 摄像头，固定机位。
- 胸部以上的头、躯干、双臂、手腕和离散手势跟踪。
- 头部朝向、眼睛开合/视线、张嘴、微笑和抬眉控制量。
- 双手“蛛丝”手势检测和确定性技能状态机。
- 原创蜘蛛主题 VRM 0.x 角色、待机动画、眨眼和 Spring Bone。
- 固定干净背景 + 人像遮罩，或完全虚拟背景。
- Pocket 最终画面窗口供 OBS 采集。
- 断网启动和运行；摄像头帧默认不落盘。

首版明确不做：

- 生成式 video-to-video。
- 语音聊天、对话或任何 LLM 功能。
- 多人跟踪。
- 无约束走动的电影级全身动捕。
- 任意真实手持物体的自动深度遮挡。
- 未经授权的蜘蛛侠模型、商标或贴图。

## 2. 验收标准

### 功能

1. `bun run live:diagnose` 验证权限/设备/协议，`bun run live:camera-smoke` 显示输入与跟踪 FPS。
2. `bun run live` 能在没有网络的情况下启动 Pocket 角色窗口。
3. 摄像头中抬头、转肩、抬臂、屈肘能驱动对应 VRM Humanoid 骨骼。
4. 眼睛开合、视线、头部朝向和张嘴能驱动模型已有的 VRM bones/expressions。
5. 关键点置信度下降时，不把 NaN、无穷值或突变姿态写入角色。
6. 蛛丝手势必须持续满足阈值后才触发，松手后经过迟滞才能再次触发。
7. 脸/躯干/左右臂独立管理；手臂丢失 750 ms 内保持最后可信姿态，之后在
   1000 ms 内平滑回到待机动画。
8. 可选择虚拟背景，或采集 clean plate 后隐藏真人。
9. OBS 能稳定采集最终 1920x1080、60 fps 画面。

### 性能

- Pocket 渲染：60 fps，连续 10 分钟 1% low 不低于 55 fps。
- 身体＋双手跟踪：目标 18–30 fps；面部 landmarks 目标 10–15 fps。
- 摄像头采集到 Pocket 姿态生效：P95 小于 80 ms。
- 摄像头采集到 OBS 预览：P95 小于 120 ms。
- 连续运行 2 小时无持续内存增长；稳定后 RSS 斜率不超过 1 MB/分钟。
- 跟踪/解算线程不能阻塞 Pocket 渲染线程。

### 本地性与隐私

- 拔网后所有核心功能仍可启动和运行。
- 运行期没有 HTTP、WebSocket、遥测或远程资源请求。
- QuickJS guest 不暴露网络 API。
- 模型、动画、配置和 shader 均来自本机受校验资产。
- 除非用户显式录制，程序不持久化原始视频帧。

## 3. 总体架构

```text
AVFoundation camera (CVPixelBuffer, up to 60 Hz)
                  |
                  +--> Preview/frame texture -----------------------+
                  |                                                  |
                  +--> 640x360 local workers                        |
                         | MediaPipe pose + 2 hands (15 Hz)          |
                         | MediaPipe face blendshapes (15 Hz)        |
                         | Apple Vision person matte (5-10 Hz)       |
                         v                                           |
                  TrackingFrame + timestamp                          |
                         |                                           |
                  Pose pipeline (60 Hz)                              |
                         | calibration                               |
                         | confidence gate                           |
                         | One-Euro / face EMA filters               |
                         | two-bone IK                               |
                         | gesture FSM                               |
                         v                                           v
                  LocalPose + GestureEvents                 Metal compositor
                         |                                           ^
                         v                                           |
             Pocket pose injection --> pocket-vrm --> pocket3d ------+
                                                                  |
                                                     final 1080p60 frame
                                                                  |
                                         Phase 1: OBS window capture
                                         Phase 3: IOSurface OBS source
```

线程之间使用有界的 latest-value mailbox。消费者只读取最新完整帧，生产者在落后时丢弃旧帧，绝不排无限队列。

## 4. 模块边界

### 4.1 `pocket-live-core`

纯 Rust、无 Apple 框架、无 GPU，负责可单元测试的实时逻辑：

- 关节名称和 `TrackingFrame` 数据契约。
- 置信度门控和跟踪生命周期。
- One-Euro 位置/旋转滤波。
- 摄像头坐标到规范角色坐标的转换。
- 校准参数。
- 骨链方向解算和 VRM Humanoid 局部旋转输出。
- 手指 curl 推导。
- 蛛丝等手势状态机。
- 录制/回放 tracking frame，便于无摄像头回归测试。

该 crate 不读取文件、不打开设备、不创建线程，给定相同输入必须生成相同输出。

### 4.2 本地视觉 bridge

本机 Apple 框架适配层：

- AVFoundation 选择设备和采集 `CVPixelBuffer`。
- 无合成消费者时使用摄像头原生 YUV；实时合成模式请求 BGRA，逐行打包到本地共享内存。
- Swift 进程只负责 AVFoundation 摄像头、person matte 和 POSIX 共享内存发布。
- Python 3.12 进程固定使用 `mediapipe==0.10.32`，在同一 640x360 帧上执行
  Face Landmarker、Pose Landmarker Lite 和 Hand Landmarker。
- Face 直接输出 52 个 blendshape 中需要的语义量和变换矩阵；Pose 输出公开的
  33 点语义骨架；Hand 每只输出 21 点。Pocket 契约只保留渲染需要的字段。
- 手腕优先采用 Hand Landmarker 端点；手可靠但肘部被遮挡时，保留 Pose 的肘部
  预测并降低置信度，避免因一个点短暂消失释放整条手臂。
- 为每份结果携带捕获时间戳，不使用处理完成时间代替。
- 跟踪压力过高时丢弃旧帧。
- 输出版本化 JSON tracking 契约，并用 POSIX shm sequence-lock 发布 BGRA/person matte。

首个集成版本允许它作为本地 Swift helper 进程运行，通过 stdin/stdout 或 Unix domain socket 与 Pocket host 通信。这能先验证权限、Vision API 和数据质量。接口稳定后再改为 C ABI 静态库并合入单进程；核心数据契约保持不变。

### 4.3 `pocket-character` host

复用现有基线：

- VRM 0.x 载入。
- VRMA 动画重定向。
- `ModelInstance::pose` 显式姿态注入。
- Morph Target、眨眼、eye look-at 和 Spring Bone。
- wgpu/Metal 窗口和固定帧率循环。

新增逻辑位于动画采样之后、眼睛和 Spring Bone 之前：

```text
sample VRMA idle locals
    -> blend tracked torso/arms into locals
    -> blend tracked head rotation
    -> tracked eye look / blink / VRM expressions
    -> spring bones
    -> globals/palette
    -> render
```

不能直接覆盖所有骨骼。未跟踪的下半身和低置信度骨骼继续使用动画；跟踪权重按状态平滑变化。

### 4.4 QuickJS policy

QuickJS 只负责低成本、可热替换的策略：

- `gestureStart` / `gestureEnd` 到动画、表情和特效的映射。
- 场景模式切换。
- 自动眼睛表情。
- 本地快捷键行为。

不把视频帧传进 guest，不在 guest 中执行 Vision、滤波、IK、分割或 GPU 合成。

### 4.5 compositor/output

MVP 提供两种背景：

1. 完全虚拟背景：最稳定，直接渲染角色和场景。
2. 固定实景：启动时采集无人 clean plate，利用 person matte 将真人区域替换为 clean plate，再渲染角色。

首版真实道具策略：固定桌面道具使用人工前景 mask；手持道具改为虚拟道具。任意物体实例分割和深度遮挡留到后续里程碑。

## 5. 数据契约

所有坐标和四元数在接口中明确约定，禁止依靠隐含引擎惯例。

```rust
pub struct TrackingFrame {
    pub schema_version: u16,
    pub sequence: u64,
    pub captured_at_ns: u64,
    pub image_size: [u32; 2],
    pub body_space: BodyCoordinateSpace,
    pub body: BodyObservation,
    pub hands: [HandObservation; 2],
    pub face: Option<FaceObservation>,
}

pub struct FaceObservation {
    pub head_rotation_radians: [f32; 3],
    pub eye_blink: [f32; 2],
    pub eye_look: [f32; 2],
    pub mouth_open: f32,
    pub smile: f32,
    pub brow_raise: f32,
    pub confidence: f32,
}

pub struct TrackedPoint3 {
    // Camera-normalized coordinates: +X right, +Y up, +Z away from camera.
    pub position: [f32; 3],
    pub confidence: f32,
}
```

进入 pose solver 后统一转换为 Pocket 模型空间。任何跨边界的数据必须满足：

- 所有浮点数为有限值。
- `confidence` 被限制在 `[0, 1]`。
- 点缺失通过 `Option` 或有效位表示，不用零向量冒充。
- frame sequence 单调递增。
- 时间戳为捕获时的单调时钟。

## 6. 姿态解算策略

### 校准

用户面对摄像头保持自然中立姿势约 2 秒，收集可信帧中位数：

- 肩宽、髋宽、躯干长度。
- 左右上臂和前臂长度。
- 摄像头到人物的参考尺度。
- 肩线和髋线的中立方向。

校准结果写入本地配置；设备、镜头或机位变化后重新校准。

### 滤波与门控

- 位置使用 One-Euro filter；快速动作提高截止频率，静止时抑制抖动。
- 旋转通过归一化四元数和最短弧插值处理。
- 单关节置信度低时沿用父链和上一可信值。
- 身体整体丢失时执行 Hold -> Recover -> Idle 状态机。
- 每帧对骨长变化和角速度设上限，拒绝明显异常值。

### 骨骼映射

首版映射：

| Vision | VRM Humanoid |
|---|---|
| root/hips | hips |
| spine/chest | spine, chest, upperChest |
| neck/head | neck, head |
| shoulder/elbow/wrist | shoulder, upperArm, lowerArm, hand |
| hand landmarks | thumb/index/middle/ring/little curl 或预制 hand pose |

手势识别和手指显示分离：手势由几何比例判定并带时间迟滞；显示时优先混合到预制的稳定手型，而不是逐个复制有噪声的手指关节。

## 7. 跟踪后端选择

默认后端是固定版本、全本地的 MediaPipe Tasks。选择理由不是跨平台，而是它直接
提供稳定的语义输出：Face Landmarker 的 blendshapes 和头部变换、Pose 的 33 个
身体点、Hand 的每手 21 点。最初采用 Apple Vision 五官二维比例和 Holistic Body
Pose 的实现已被实测否决：半身近景时肘腕召回不足，二维距离也不能稳定表示眨眼、
张嘴和微笑。

Apple 框架仍承担最适合它的部分：AVFoundation 零拷贝采集和 Vision person matte。
三套 MediaPipe task 共享一次缩放后的帧，不重复打开摄像头；Swift 的身体、手和脸
请求在默认配置下关闭。模型由 `assets/manifest.json` 固定大小与 SHA-256，运行期
完全断网。后续可在不改变 TrackingFrame v3 的情况下把 Python adapter 替换为
Rust/C++ C ABI，但不应改变语义控制层。

## 8. 性能与延迟设计

建议预算：

| 阶段 | 目标 P95 |
|---|---:|
| 摄像头帧到达 | 8–17 ms |
| 身体/手部 Vision | 10–30 ms，异步 |
| 面部 landmarks | 10–15 Hz，复用 face ROI |
| pose filter + IK | < 1 ms |
| Pocket 角色更新和渲染 | < 8 ms |
| OBS 获取最终帧 | 8–17 ms |
| VideoToolbox 编码 | 5–15 ms |

关键规则：

- 渲染 60 Hz，Vision 不必 60 Hz。
- body+hands、face、matte 采用不同频率。
- 1080p 只用于最终画面；Vision 默认处理 640x360 inference buffer。
- segmentation 使用较低分辨率后在 GPU 上采样和羽化。
- `CVPixelBuffer -> CVMetalTexture` 尽量零拷贝。
- 性能统计用捕获时间戳测量真实 age，不能只测函数耗时。

当前机器真实摄像头短测（2026-08-19，MacBook Pro Camera 1920x1080@30）：

| 指标 | 实测 |
|---|---:|
| Pocket 最终画面 | 60.00 fps |
| 1% low | 56.70 fps |
| Holistic 身体＋双手 | 19.70 fps |
| 5 秒窗口内有效 face frame | 35 |
| shared camera / person matte frame | 112 / 111 |
| 协议拒绝帧 | 0 |
| host + Vision helper 瞬时 CPU | 约 42% 单核（100% = 1 核） |
| host + Vision helper RSS | 约 774 MB |

短测只证明基础性能和各通路没有互相阻塞；10 分钟 1% low、端到端 P95 和 2 小时
内存斜率仍需在最终摄像头、VRM 和 OBS 组合上验收。

## 9. 离线与供应链

开发首次安装可以联网获取 Rust/JS 依赖和示例资产；生产运行不得联网。达到可离线构建还需：

- 固定 `Cargo.lock`、PocketJS submodule commit 和 Bun lockfile。
- 将允许再分发的 crates/npm 包 vendor 到发布构建环境。
- 原创 VRM/VRMA/shader 直接随 App Bundle 发布。
- 建立资产 `manifest.json`，记录相对路径、大小、许可证和 SHA-256。
- 启动时校验资产，不在校验失败时尝试远程下载。
- 发布包执行断网烟雾测试。

## 10. OBS 路径

### MVP：窗口采集

- Pocket 渲染一个 1920x1080、60 fps 的最终合成窗口。
- OBS 使用 macOS Window Capture。
- OBS 使用 Apple VT H.264 Hardware Encoder。

这条路径先验证功能、画质和延迟。

### 生产：IOSurface source

- Pocket 将最终 Metal texture 发布到共享 IOSurface。
- 本地控制通道只发送 surface ID、尺寸、格式和时间戳。
- 自定义 OBS source plugin 直接导入 IOSurface。
- 无订阅者时停止发布，OBS 重启后能够重新发现 source。

## 11. 里程碑

### M0：基线和方案

- [x] 引入 Pocket Character 和固定 PocketJS 子模块。
- [x] 写明架构、契约、验收标准和风险边界。
- [x] 构建 guest、Rust tests 和 headless screenshot。

### M1：可测试的 tracking/pose core

- [x] 建立 `pocket-live-core`。
- [x] 实现 tracking schema、有限值校验和 latest-value mailbox。
- [x] 实现 One-Euro filter。
- [x] 实现 tracking lifecycle 和蛛丝手势 FSM。
- [x] 用合成数据和 mock bridge 完成首批单元/渲染测试。

### M2：本地摄像头和 Vision diagnostic

- [x] 建立 Swift `PocketVisionBridge` helper。
- [x] 完成摄像头权限、设备枚举和 1080p60 请求。
- [x] 输出 2D 身体、左右手和捕获时间戳。
- [x] macOS 15 Holistic 身体＋双手单请求，macOS 14 保留兼容回退。
- [x] Face Landmarks 输出头部、眼睛、嘴、眉毛控制量并复用 ROI。
- [x] 加入 3D body pose，并在无完整身体时回退 2D。
- [x] 加入 person matte + BGRA POSIX shm 契约；IOSurface 留给生产输出阶段。
- [x] 提供纯本地 diagnostic CLI 和确定性 mock fixture。
- [ ] 加入真实 tracking frame 录制/回放 fixture。

### M3：Pocket 角色驱动

- [x] host 接收最新 tracking frame。
- [x] 完成首版 60 帧中位数校准和双臂骨骼旋转解算。
- [x] 将校准尺度用于躯干与手臂异常长度门控，并加入胸、颈、头方向解算。
- [x] 首版双臂方向解算并在 VRMA 与 tracking 之间按骨骼混合。
- [x] 把手势事件暴露给 QuickJS。
- [x] 头部旋转、eye look、blink、张嘴/微笑/抬眉映射到 VRM。
- [x] 跟踪丢失后保持并平滑回到 idle。

### M4：合成和直播

- [x] 虚拟背景与摄像头背景 1080p60。
- [x] clean plate/person matte 及 matte+虚拟背景合成。
- [x] 输出固定 1920x1080、60 fps 窗口并提供 OBS 采集配置；当前机器未安装 OBS，应用内路径已完成。
- [ ] 描边、色阶量化、网点和蛛丝特效。

### M5：生产化

- [ ] IOSurface OBS source。
- [x] 资产 SHA-256 清单、构建和诊断时校验。
- [x] 断网审计与基础 GPU/真实摄像头性能基准自动化。
- [ ] 真实端到端延迟和 2 小时长稳在最终摄像机/角色/OBS 组合上验收。
- [ ] macOS App Bundle、权限说明和本地诊断包。

## 12. 主要风险和降级策略

| 风险 | 处理 |
|---|---|
| 单目深度造成手臂翻转 | 半身限制、校准、肘部 pole vector、角速度门控 |
| 双手靠近脸或互相遮挡 | 降低置信度并保持上一手型，不猜测穿越姿态 |
| Vision 跟不上 60 fps | 采集/渲染 60，Vision 15–30，永远只消费最新帧 |
| 真人轮廓露边 | mask 腐蚀、羽化、时序平滑；优先虚拟背景 |
| 实物遮挡穿帮 | MVP 使用虚拟道具或固定前景 mask |
| VRM 坐标系不一致 | 使用显式坐标契约和渲染 fixture 回归测试 |
| OBS 窗口捕获增加延迟 | 功能稳定后替换为 IOSurface source |
| 示例资产不能商业发布 | 开发用官方样例；发布前换原创并记录许可证 |

## 13. 完成定义

本次 Codex MVP 目标以以下条件为完成：真实摄像头的身体/双手/脸/人像遮罩能进入
Pocket；VRM 骨骼和表情有明确驱动路径；最终窗口为 1080p60；构建、单元测试、
mock/真实摄像头烟测、离线审计和基础性能测试通过；OBS 配置有可执行文档。本次均已
满足。单独跑通角色、单独打印 Vision 关键点或只提供设计文档不算完成。

商业上线仍需在最终摄像头、原创角色和已安装 OBS 的机器上完成 10 分钟 1% low、
端到端 P95、2 小时内存斜率和平台推流验证；这些是生产发布门，不反向阻塞本次
工程 MVP 的完成状态。
