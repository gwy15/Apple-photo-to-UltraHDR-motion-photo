# GLOSSARY

项目领域术语表。架构评审与文档里的措辞以此为准（配合 `/codebase-design` 的 module/interface/seam/depth 词汇）。

## 任务发现 (task discovery)

遍历一个目录树，把其中的照片与视频**配对**成转换任务的过程。实现在 `src/discover.rs`，公共 seam 只有一个入口：`discover(root, &DiscoverOptions) -> Result<Discovery>`。转换参数（质量、exiftool 路径等）不过这个 interface——那属于转换，不属于发现。

## 配对 (pairing)

同一个 stem（文件名去扩展名）下的图与视频组成一个转换任务的规则。要点：

- 图片候选按优先级排序：heic（Apple HDR 源）优先，其余次之，同级按路径排序保证确定性；
- 每个 stem 只建一个任务：有视频时由「迭代到视频」触发，无视频（静态照片）时由「迭代到图片」触发——否则 -j 并行下会同时写同一个输出文件；
- 输出名 = stem + `jpg`（可加 output_suffix）。

## 静态照片 (static photo)

没有配对视频的照片。`DiscoveredPhoto.video` 为 `None`；转换时跳过动态照片追加，但仍写入 HDR（heic 源）与元数据。

## DiscoveryIssue

发现阶段的非致命问题（重名被忽略、symlink 跳过、非 heic 跳过、坏文件），按 `kind` 枚举断言而非匹配字符串。根目录不存在/不是目录不属于 issue，是硬错误。

## 动态照片 (motion photo)

Android (GCamera XMP) 语义：JPEG 数据末尾追加视频字节，`MicroVideoOffset` 记录追加的实际字节数。对应 Apple 的 Live Photo 源（HEIC + MOV）。

## Apple HDR gainmap

HEIC 主图之外的辅助图（`urn:com:apple:photo:2020:aux:hdrgainmap`）+ headroom 元数据。转换核心：把它换算成 UltraHDR 的 log_recovery 增益图（见 `src/i2a/convert.rs` 的注释与引用链接）。
