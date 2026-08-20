# Pocket Live 本地插件

Pocket Live 把实时跟踪/渲染核心与可替换内容分开。核心只认识两种 manifest：

- `character`：VRM、待机 VRMA、QuickJS 角色策略，以及该模型的贴图和镜头参数。
- `background`：背景 WGSL、默认合成模式，以及 clean-plate 延迟。

插件是普通本地目录，不下载代码、不访问外部服务，也不依赖 LLM。所有相对路径都以
`plugin.json` 所在目录为基准。当前效果分别位于
`plugins/characters/default` 和 `plugins/backgrounds/default`，它们只是内置默认插件，
不是核心特例。

## 选择插件

```sh
# 默认人物 + 默认漫画背景
bun run live

# 换背景；无需重编译 Rust
bun run live -- \
  --background-plugin /absolute/path/to/background/plugin.json

# 构建并运行另一人物插件
bun run build:ui -- \
  --character-plugin /absolute/path/to/character/plugin.json
bun run live -- \
  --character-plugin /absolute/path/to/character/plugin.json

# 仓库自带的第二个背景用于验证可替换性
bun run live -- \
  --background-plugin plugins/backgrounds/studio/plugin.json
```

`--model`、`--vrma`、`--bundle`、`--background` 和 `--clean-plate-delay` 仍保留，
但只作为 manifest 之上的临时覆盖参数。长期配置应该写进插件。

仓库还包含一组完整的原创示例：

```sh
bun run characters:build
bun run live:golden-horn
```

它组合 `plugins/characters/golden-horn` 的程序化牛形 VRM 与
`plugins/backgrounds/golden-sunset` 的落日荒原 shader。生成的 VRM 不提交进仓库，
`setup` 和 `live:build` 会在本地确定性重建。

## 人物插件契约

```json
{
  "schema_version": 1,
  "kind": "character",
  "id": "example.hero",
  "name": "Example Hero",
  "model": "assets/hero.vrm",
  "idle_animation": "assets/idle.vrma",
  "policy": {
    "entry": "main.ts",
    "bundle": "dist/character.js"
  },
  "render": {
    "max_texture_dimension": 2048,
    "fov_y_degrees": 40,
    "anchor_height_ratio": 0.72,
    "camera_distance": 1,
    "split_camera_distance": 1.35
  }
}
```

`main.ts` 从 `plugin-sdk/character.ts` 导入稳定的角色 API。它只处理角色策略，例如选择
动画、点击反应或手势事件；摄像头、MediaPipe/Vision、滤波、IK、渲染循环都仍在原生
核心中。宿主启动前，`policy.bundle` 必须已由 `bun run build:ui` 生成。

人物模型目前要求 VRM 0.x。可以先运行：

```sh
target/release/pocket-character --model-info /absolute/path/to/hero.vrm
```

## 背景插件契约

```json
{
  "schema_version": 1,
  "kind": "background",
  "id": "example.background",
  "name": "Example Background",
  "shader": "background.wgsl",
  "default_mode": "virtual",
  "clean_plate_delay_seconds": 3
}
```

背景 shader 只需实现一个纯函数：

```wgsl
fn plugin_background(uv: vec2f, time: f32) -> vec3f {
    return vec3f(uv.x, uv.y, 0.2 + 0.1 * sin(time));
}
```

核心负责摄像头纹理、人像 mask、clean plate、split 布局和最终混合；插件只决定背景
像素。可用的 `default_mode` 为 `transparent`、`virtual`、`camera`、`matte`、
`clean`、`split`。即使使用 `matte` 或 `clean`，背景外观仍来自同一个插件函数。

## 校验和失败策略

```sh
bun run plugins:verify
bun scripts/verify-plugins.ts --runtime
```

加载器会拒绝错误 schema/kind、重复或不安全的 id、丢失的文件、越界的镜头参数、
未知的合成模式，以及没有 `plugin_background` 入口的 shader。WGSL 最终还会经过 wgpu
编译验证。插件是本机可信内容；它们不会被隔离执行，因此只应加载自己创建或审查过的
策略代码和 shader。
