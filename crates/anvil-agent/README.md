# anvil-agent

与游戏 SDK 无关的 AI 玩家本地原型，尚未接入 Minecraft、Azalea 或 LLM。

- `Executor` 为既有确定性技能执行器，覆盖跟随、数量型采集、避险及取消确认。
- `contract` 为下一阶段的目标接纳、跟随阶段转换、决策来源校验和控制权契约原型。它不调度动作，当前与 `Executor` 独立，不能将两者并行接入一个控制通道。

完整约定与接入边界见[跟随契约](../../docs/ai-player/follow-contract.md)，阶段计划见[AI 玩家执行项](../../todos/ai-player.md)。

离线验证使用人工时钟和模拟事件。

```sh
cargo test -p anvil-agent --offline
cargo fmt --all -- --check
```
