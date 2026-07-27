# Ora 插件管理线协议与粘包分包分析

> 状态：一手代码审计（全部 cited file:line，本会话内可复现）
> 日期：2026-07-25
> 范围：`crates/plugin-protocol`（Rust 线协议）+ `packages/plugin-runtime`（TS bootstrap 线协议）+ `crates/plugin-manager/src/runtime/transport.rs`（宿主读写循环）+ `crates/process`（进程管道）
> 问题：当前插件管理（含 sdk/protocol/runtime）能否支持"选择 agent 插件后与之对话"？实现是否合理？是否存在粘包/分包问题？宿主↔插件进程用什么帧格式通信？是否合理？
> 结论速览：**选择 Agent 插件→对话端到端可用**（引擎层真实 Bun+Job e2e 已证明，见 `2026-07-25-plugin-manager-backend-capabilities-analysis.md` §6.7）；**帧格式 = 5 字节头 `[type:i8][length:i32 BE][payload]` 的长度前缀二进制分帧**，两端编解码器逐字节对齐；**粘包/分包已被彻底解决**（增量状态机解码器 + 全 split 位测试 + 合帧测试）；**整体实现合理**，对标 LSP/gRPC 式成熟模式，并叠加了严格 JSON、deny-unknown-fields、no-batch、boundary-strict EOF、no-resync、writer 三 lane 背压等加固。本文件不重复评估 scan/validate/install/execute，那部分见前一份分析。

---

## 1. 问题与判定速览

| 问题 | 判定 | 证据锚点 |
|---|---|---|
| 选 Agent 插件后能对话吗？ | ✅ 能 | `plugin_library_e2e.rs` 真实 Bun 跑通 startConversation→ConversationStarted→TextDelta→AgentTurnResult + sendMessage+cancel（见前文 §6.7） |
| 帧格式 | 5B 头 `[type:i8][length:i32 BE][payload]`，type 仅 `1=Json`，payload 上限 8 MiB | `frame.rs:FRAME_HEADER_BYTES/MAX_FRAME_BYTES`；`frame.ts` 同 |
| 粘包（一读多帧） | ✅ 已解决 | `decode_chunk` 一次返回 `Vec<Frame>`，while 循环消费完 chunk；合帧测试 `decodes_arbitrary_splits_and_coalesced_frames` |
| 分包（一帧多读） | ✅ 已解决 | `DecoderState::{Header,Payload}` 跨 chunk 持续累积，header/payload 任一半可断在任意字节；全 split 位测试 |
| 半帧 EOF | ✅ 拒绝（fail-closed） | `finish()` 在非 Header{filled:0} 状态返 `PartialFrame`；reader `transport.rs` 转 `BoundaryEof`/`Failure` |
| 错误后重同步 | ✅ 不做（安全） | 解码错即 fatal，**不尝试字节流重同步**——二进制分帧 JSON-RPC 流上重同步不安全 |
| 编解码两端一致 | ✅ 逐字节对齐 | Rust `frame.rs` 与 TS `frame.ts` 同 5B/BE i32/8MiB/同校验顺序 |
| 整体合理性 | ✅ 合理 | 见 §5 评估 |

---

## 2. 帧格式（一手定义）

### 2.1 头部布局

`crates/plugin-protocol/src/frame.rs`：

```
偏移  长度  字段        编码
0     1    type       i8（有符号！），v1 仅 1=Json
1     4    length     i32 Big-Endian（有符号！），= payload 字节数，必须 >0
5     N    payload    N=length，≤ MAX_FRAME_BYTES
```

- `FRAME_HEADER_BYTES = 5`（`frame.rs:FRAME_HEADER_BYTES`）。
- `MAX_FRAME_BYTES = 8 * 1024 * 1024`（8 MiB，`frame.rs:MAX_FRAME_BYTES`），**payload 上限**，校验在分配前（`frame.rs:validate_payload_length`）。
- `FrameType::Json = 1`，`TryFrom<i8>` 对未知值返 `UnknownType`（`frame.rs`）。
- **length 是 i32**：≤0 一律 `NonPositiveLength`（`frame.rs:validate_payload_length` + 解码侧 `length <= 0` 分支，覆盖 `0x00000000` 与负数 `0xFFFFFFFF=-1`）。测试 `rejects_invalid_headers_and_partial_eof` 覆盖 `length=0`、`length=-1`、`type=127`。

### 2.2 编码（写出）

`encode_frame`（`frame.rs`）：`Vec::with_capacity(5 + payload.len())` → `push(type as i8 as u8)` → `extend_from_slice(&length.to_be_bytes())` → `extend_from_slice(payload)`。无填充、无分隔符、无换行——**纯长度前缀**。

### 2.3 两端镜像

`packages/plugin-runtime/src/transport/frame.ts:encodeFrame`：
```ts
view.setInt8(0, type);              // 有符号
view.setInt32(1, payload.byteLength, false); // false = big-endian
encoded.set(payload, 5);
```
`crates/plugin-protocol/src/frame.rs:encode_frame` 用 `length.to_be_bytes()`。**两者都 Big-Endian、都 i8/i32 有符号语义、都 8MiB、都校验 positive + cap + known-type，且校验顺序一致（先 maximum 配置、再 length、再 type）**。golden 测试 `encodes_canonical_golden_vector` 断言头部 `[0x01, 0x00, 0x00, 0x00, 0x38]`（type=1, length=0x38=56）。

---

## 3. 粘包/分包分析（核心问题）

### 3.1 解码器状态机

`FrameDecoder`（`frame.rs`）持有 `DecoderState`：

```rust
enum DecoderState {
    Header { bytes: [u8;5], filled: usize },
    Payload { frame_type, expected: usize, bytes: Vec<u8> },
}
```

`decode_chunk(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, FrameError>`（`frame.rs`）核心循环：

- **Header 态**：`remaining = 5 - filled`；`copied = remaining.min(chunk.len())`；把 `chunk[..copied]` 拷进 `bytes[filled..]`；`filled += copied`；`chunk = &chunk[copied..]`。若 `filled==5` → 解析 type/length，校验，转 Payload 态。
- **Payload 态**：`remaining = expected - bytes.len()`；`copied = remaining.min(chunk.len())`；`bytes.extend(..)`；`chunk = &chunk[copied..]`。若 `bytes.len()==expected` → `mem::take` 出 payload，push 一帧，**回到空 Header 态**。
- 循环 `while !chunk.is_empty()`：**一个 chunk 里若含多个完整帧，全部产出**（解决"粘包"）；**一帧跨多个 chunk 则跨调用累积**（解决"分包"）。

关键不变量：
- payload 缓冲只在**校验 length 合法后**才分配 `Vec::with_capacity(expected)`（`frame.rs`，注释"never buffers more than one payload"）——防放大攻击/越界分配。
- 每帧完成即复位到空 Header，无残留。

### 3.2 测试覆盖（决定性）

`frame.rs:decodes_arbitrary_splits_and_coalesced_frames`（`frame.rs` 测试）：

1. **全 split 位**：`for cut in 0..=encoded.len()`，把一帧切成 `[..cut]` + `[cut..]` 两次喂给同一 decoder，断言产出一帧且 `finish()==Ok(())`。这覆盖：header 在任意字节断开、header/payload 边界断开、payload 在任意字节断开。
2. **合帧（粘包）**：`doubled = encoded ++ encoded`，一次喂入，断言产出**两帧**且顺序正确。

即"分包"与"粘包"两条退化路径都有显式测试。另有 `rejects_invalid_headers_and_partial_eof` 覆盖 `length=0`/`-1`/`type=127`/半帧 EOF。

### 3.3 TS 侧对称

`frame.ts:FrameDecoder.decodeChunk`（`packages/plugin-runtime/src/transport/frame.ts`）：同样的 Header/Payload 双态、`Math.min` 跨 chunk 拷贝、一 chunk 多帧 push、完成复位。`finish()` 同样在非空状态抛 `partial frame`。

### 3.4 为什么这彻底解决粘包/分包

字节流（pipe/socket）的语义是：**read 返回的字节边界与消息边界无关**。两类问题：
- **粘包**：一次 read 拿到 >1 条消息的字节。→ 长度前缀让解码器知道第 1 条的边界，消费后**继续用剩余字节**解析第 2 条（while 循环）。
- **分包**：一次 read 拿到 <1 条消息的字节。→ 解码器**保留已读的 header/payload 半成品状态**，下次 read 续上。

这正是 length-prefixed framing 的标准做法（对比：纯 JSON 文本流若靠换行/括号计数分帧才真有歧义与重解析成本）。Ora 选的是二进制长度前缀，**不存在**"找不到边界"的歧义。

---

## 4. 宿主读写循环（runtime/transport.rs）

### 4.1 reader

`spawn_reader`（`crates/plugin-manager/src/runtime/transport.rs:313`）→ `run_reader`（`transport.rs:326`）：

```rust
let mut decoder = FrameDecoder::new(MAX_FRAME_BYTES)?;   // transport.rs:330
let mut chunk = vec![0u8; READ_CHUNK_BYTES];              // 固定缓冲
loop {
    match stdout.read(&mut chunk).await {
        Ok(0) => { // EOF
            match decoder.finish() {                       // transport.rs:333
                Ok(()) => events.send(BoundaryEof),        // 必须停在帧边界
                Err(_)  => events.send(Failure(InvalidFrame)),
            }; return;
        }
        Ok(n) => {
            let frames = decoder.decode_chunk(&chunk[..n])?;  // transport.rs:357
            for frame in frames {                              // 一读多帧全派发
                let env = parse_json_rpc_frame(&frame, max_depth)?;  // 严格 JSON-RPC
                events.send(Envelope(env)).await;             // 背压：满则等
            }
        }
        Err(e) => events.send(Failure(Io(classify_io_error(e)))),
    }
}
```

要点：
- 用**同一个** `FrameDecoder`/`MAX_FRAME_BYTES`，与 TS 侧逐字节对齐。
- `decode_chunk` 出 0..N 帧，逐帧 `parse_json_rpc_frame` 后通过 mpsc 派发——**mpsc 满时 await 背压**，不会丢帧或越界。
- **EOF 必须在帧边界**：`finish()` 非 `Ok` → `InvalidFrame` fatal（`transport.rs:333-338`）。这防"半条消息被当完整消息"。
- 任一解码/解析错 → 发 `Failure` 后 `return`，**不再读流**（fail-closed，见 §3 "不重同步"）。

### 4.2 writer

`spawn_writer`（`transport.rs:187`）持 `WriterQueues`（lane 化的 FIFO）。`WriterQueues::enqueue(generation, owner, payload, lane, deadline)`（`transport.rs`）：
- `encode_json_frame(payload, MAX_FRAME_BYTES)`（`transport.rs:148`）—— 一帧 = 5B 头 + payload，**整帧一次 write**（不拆）。
- 写完成发 `WriterCompletion::FrameWritten{generation, owner}`（`transport.rs`）——给 handshake/actor 的"response-after-writer-ack"因果序用（`handshake.rs:lifecycle_round_trip`）。
- 三 lane（`WriterLane::{Ordinary,TransportCancel,SessionControl}`）：普通请求与 cancel/控制分 lane 背压，防普通流量挤掉控制帧。

### 4.3 TS 侧 writer

`packages/plugin-runtime/src/transport/writer.ts:ProtocolWriter`：
- `enqueue(payload, lane)`：`encodeJsonFrame(payload)` → `#reserve`（总量 + 普通/safety 预留预算）→ 串到 `#tail` 链后 `#writeFrame`。
- `#writeFrame`：`stdout.write(frame, callback)`——**整帧一次 write**，回调确认后才 `#release`。
- `maximumFrames`/`maximumBytes`/`reservedControl*`/`reservedSafety*` 预算（`writer.ts:ProtocolWriterLimits`）—— control lane 永不被普通流量饿死（lifecycle/$/stream/cancel 能出去）。
- `validateWriterLimits`（`writer.ts`）：reserved 之和必须 < maximum（留普通容量）。

---

## 5. 合理性评估

### 5.1 帧格式合理吗？—— 合理

- **长度前缀二进制分帧**是成熟工业模式：LSP（Language Server Protocol）官方 transports 里就有 Content-Length 头的"类长度前缀"做法，但那是文本头、易错；Ora 用**纯二进制 5B 头**，更紧凑、解析更快、无歧义。对标 gRPC over HTTP/2 的 length-prefixed 帧、Tauri IPC 的二进制前缀。
- `type` 字节预留扩展位（未来可加二进制 payload kind 而不复用 JSON），但 v1 只开 `1=Json`，未知即拒——**前向兼容有路、当前 fail-closed**。
- `length` 用 i32 BE（有符号）：i32 上限 ~2GiB，实际 cap 8MiB，双层防御；有符号语义让 `<=0` 一律非法，**消除"零长度帧=空消息"歧义**。
- 8MiB payload cap + 分配前校验：防放大/拒绝服务。
- **无换行/无分隔符依赖**：JSON 里出现 `\n` 或任意字节都不影响分帧（payload 是原始字节，不按文本切）。

### 5.2 粘包/分包处理合理吗？—— 合理

- 状态机式增量解码是教科书做法，且**两端镜像**（Rust `frame.rs` 与 TS `frame.ts` 行为等价、常量等价、校验顺序等价）。
- 测试覆盖**全 split 位 + 合帧 + 半帧 EOF + 非法 header**——这是能想到的最严边界覆盖。
- 不缓冲无界数据：任一时刻最多持**一个 payload 的半成品**（`frame.rs` 注释 "never buffers more than one payload"）。
- EOF 在非帧边界即 fatal：防"半条消息被当完整"。
- **不重同步**：二进制分帧的 JSON-RPC 流上，一旦失步无法安全找回边界（不像文本协议可扫分隔符），所以 fail-closed 是唯一安全选择。Ora 显式这么做（`FrameError` 后 reader `return`）。

### 5.3 JSON-RPC 严格 profile 合理吗？—— 合理且加固

`crates/plugin-protocol/src/json_rpc.rs` + `strict_json.rs`：

- **严格 JSON**（`strict_json.rs:parse_strict_json`）：**拒重复键**（`StrictJsonError::DuplicateKey`，serde 默认会静默保留最后一个——这是真实安全/语义坑，Ora 显式堵）、**拒超深嵌套**（`DepthLimitExceeded`，默认 64）、**拒尾部多余字节**（`deserializer.end()`）。
- **严格 JSON-RPC envelope**（`json_rpc.rs:parse_json_rpc_frame`）：
  - `jsonrpc` 必须 == `"2.0"`。
  - **禁 batch**（`EnvelopeNotObject`：非 object 即拒）。
  - envelope kind **由 JSON shape 判别**（有 `method`→Request/Notification；无→Response），wire type 字节只标 payload 编码、不参与方向——干净。
  - **deny_unknown_fields** 等价：`require_allowed_fields` 对每种 shape 有闭字段集（Request=`{jsonrpc,id,method,params}`、Response=`{jsonrpc,id,result,error}`、Notification=`{jsonrpc,method,params}`），未知顶层键 → `UnknownField`。
  - **cross-shape 字段先于 unknown 判别**（`parse_request`/`parse_response`/`parse_notification` 先查 `FrameEnvelopeMismatch`）—— `method+result` 这种被归为"形状冲突"而非"未知字段"，便于上层分类。
  - **id 严格**：Request id `RpcId`（非空 ≤128B），Response/Host id 必须是 `h:<u64>` 规范形（`HostRequestId::parse`，拒前导零、拒非数字、上限 `JSON_SAFE_U64_MAX`）——**防 id 伪造/复用/歧义**。
  - **params 必须是 object 或缺省**，**显式 null 被拒**（`InvalidParamsShape` + `deserialize_optional_non_null`）。
  - Response 必须恰好 `result` 或 `error` 之一（`InvalidResponseShape`），`result+error` 同存即拒。

TS 侧 `packages/plugin-runtime/src/rpc/envelope.ts:parseInboundEnvelope` 镜像：plain object 校验（拒 `__proto__` 注入）、`jsonrpc==="2.0"`、exact-keys、bounded string（含拒 NUL）、params 必 object。**宿主不向插件发 Response**（`parseInboundEnvelope` 拒 Response shape）——方向单向。

### 5.4 端到端一致性

- Rust reader 用 Rust `FrameDecoder` + `parse_json_rpc_frame`；TS bootstrap 用 TS `FrameDecoder` + `parseInboundEnvelope`。**两边常量逐一对齐**（5B / BE i32 / 8MiB / type=1 / jsonrpc=2.0 / deny-unknown / no-batch / no-null / h:<n> id）。这就是为什么真实 e2e（`plugin_library_e2e.rs`）能跑通——任何一边错配都会 fatal。
- 写侧：Rust `encode_json_frame`/`encode_json_rpc_request` 与 TS `encodeJsonFrame`/`encodeSuccess`/`encodeError`/`encodeStream` 对齐。

### 5.5 唯一可商榷点（不影响正确性）

- `length` 用 **i32**（而非 u32）：i32 负值被 `<=0` 拒，等价于"有效 payload ∈ [1, 2^31-1]"，但实际 cap 8MiB，所以 i32 vs u32 无实际差。选 i32 可能是为"负值即非法"的语义自洽，可接受。
- 无帧内分片（不支持"一个大 JSON 跨多帧"）：8MiB 内的单 JSON-RPC 消息足够（一个 `AgentEvent`/`AgentTurnResult` 远小于此）。如未来要传超大 payload（如文件），`FrameType` 已预留扩展位加二进制流帧——**不需要破坏现有 ABI**。

---

## 6. 与"选 Agent 插件→对话"的关系

帧协议是 ②↔③ 的通道（见前文 §2 分层图）。一次对话的完整字节流：

```
②宿主                           ③插件进程(Bun)
  │                                │
  │ ──帧: $/initialize Request──► │  (5B头+JSON-RPC)
  │ ◄──帧: initialize Response──  │
  │ ──帧: $/activate Request───►  │
  │ ◄──帧: activate Response───   │
  │ ──帧: agent.startConversation─►
  │ ◄──帧: $/stream {ConversationStarted}── (notification)
  │ ◄──帧: $/stream {TextDelta}──
  │ ◄──帧: agent.startConversation Success(AgentTurnResult)──
  │ ──帧: $/deactivate Request──►
  │ ──帧: $/exit notification──► │
```

每一行都是一个 5B 头 + JSON-RPC payload 的完整帧。`$/stream` 是 notification（无 id），seq 单调（`session_actor.rs:process_stream` 校验 `next_stream_sequence`）。终端 `AgentTurnResult` 是对应 Request 的 Success Response（`session_actor.rs:process_terminal`）。这条链在 `plugin_library_e2e.rs` 真实跑通（见前文 §6.7 的断言）。**帧协议本身不构成对话能力的瓶颈**——瓶颈在 ③ 内的 ACP 桥（WIP，未提交）接真实 agent CLI。

---

## 7. 复现锚点

- 帧编解码单元测试：`cargo test -p ora-plugin-protocol --lib frame`（`decodes_arbitrary_splits_and_coalesced_frames`/`rejects_invalid_headers_and_partial_eof`/`encodes_canonical_golden_vector`）。
- JSON-RPC 严格 profile：`cargo test -p ora-plugin-protocol --lib json_rpc`（`classifies_cross_shape_fields_as_envelope_mismatch`/`classifies_unknown_top_level_fields_separately`/`parses_strict_response_envelopes`）。
- 严格 JSON：`cargo test -p ora-plugin-protocol --lib strict_json`（`rejects_duplicate_keys_at_any_depth`/`enforces_nesting_depth`）。
- 端到端真实帧流：`cargo test -p ora-plugin-manager --test plugin_library_e2e -- --ignored`（前置 `task prepare-plugin-runtime`）。

---

## 8. 文件索引（一手源）

| 关注点 | 文件 |
|---|---|
| 帧编解码（Rust） | `crates/plugin-protocol/src/frame.rs` |
| 严格 JSON-RPC envelope（Rust） | `crates/plugin-protocol/src/json_rpc.rs` |
| 严格 JSON（Rust） | `crates/plugin-protocol/src/strict_json.rs` |
| lifecycle 常量/DTO | `crates/plugin-protocol/src/lifecycle.rs` + `agent/{method,dto,leaf,validation}.rs` |
| 宿主读写循环 | `crates/plugin-manager/src/runtime/transport.rs`（`spawn_reader:313`/`run_reader:326`/`spawn_writer:187`） |
| 帧编解码（TS） | `packages/plugin-runtime/src/transport/frame.ts` |
| writer lane 背压（TS） | `packages/plugin-runtime/src/transport/writer.ts` |
| JSON-RPC envelope（TS） | `packages/plugin-runtime/src/rpc/envelope.ts` |
| 严格 JSON（TS） | `packages/plugin-runtime/src/json/strict.ts` |
| bootstrap 会话分派 | `packages/plugin-runtime/src/bootstrap/session.ts` |
