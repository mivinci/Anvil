# 实时策略实验

状态为模拟环境与分组数据生成已实现，首轮 XGBoost 训练与闭环评估已运行。先验证 XGBoost 候选动作排序，按同一闭环评估结果决定是否尝试 Transformer。

## 环境

使用 Python 3.13 和 uv。macOS 需先安装 OpenMP 运行库（`brew install libomp`）。从本目录运行命令。

```sh
uv sync --locked
uv run python -c "import xgboost; print(xgboost.__version__)"
```

依赖由 `pyproject.toml` 声明，`uv.lock` 固定解析结果。生成文件写入 `outputs/`。

## 第一轮执行项

1. [x] 建立 uv 环境和实验目录。
2. [x] 定义并实现可重置、可克隆的模拟环境，覆盖跟随、采集和避险，包含位置、障碍和任务进度。
3. [x] 从同一状态执行不同候选，使用固定后续策略和有限时间窗口评价结果，生成分组排序数据。评分来源与后续策略必须记录。
4. [x] 按地图和完整 episode 固定训练、验证、测试划分，训练 XGBoost 排序模型。
5. [x] 在独立场景闭环比较模型和规则基线，记录成功率、受伤率、耗时及动作切换次数。
6. [ ] 通过真实游戏适配器验证观察与动作对应关系，补充真实数据和评估。

模拟结果只能支持模拟环境内的结论。Transformer 实验沿用数据版本、场景划分和评估协议，模型版本及特征配置分别记录。

## 目录

- `scenarios/` 保存模拟环境及场景定义。
- `dataset/` 保存数据格式、特征提取和生成逻辑。
- `xgboost/` 保存训练与推理入口，第三方模型库仍通过 `import xgboost` 使用。
- `evaluation/` 保存闭环运行和指标统计。
- `outputs/` 保存生成数据、模型和评估报告，忽略提交。

## 生成与检查

从本目录运行，每次生成需要指定一个尚不存在的输出目录。

```sh
uv run python -m unittest discover -s tests -v
uv run python -m dataset.generate --episodes 30 --horizon 8 --output outputs/dataset-v1
```

每个划分生成 30 局，按跟随、采集、撤离轮换。种子段分别从 0、1000000、2000000 开始，并检查地图摘要不重复。每次决策对所有有效候选从相同状态执行，再由相同基线策略运行到 8 步窗口结束或 episode 终止。每组按回报从低到高转为整数相关度，保留平分。manifest 保存奖励、种子段、后续策略版本和结束原因统计。

## 模拟边界

地图为 12×12，四向移动、等待和相邻采集各耗一步。观察覆盖自身周围半径 4 的正方形，墙不遮挡观察，视野外标记未知。固定一只静止苦力怕在第 8 步爆炸，距离 2 格内致死。玩家沿由种子确定的路线移动，路线不受 Bot 动作影响。

跟随累计 12 步处于玩家 2 格内完成，采集获得 3 单位资源完成，撤离在爆炸后存活即完成。每局至多 80 步。当前未模拟高度、真实物理、视线遮挡及多重威胁。特征包含局部地形、实体相对坐标、任务、上次动作、静止步数与已访问格子。

## 训练与评估

```sh
uv run python xgboost/train.py --dataset outputs/dataset-v1 --output outputs/xgboost-v1
uv run python -m evaluation.run --model outputs/xgboost-v1 --output outputs/evaluation-v1
```

训练只读取训练与验证 JSONL，验证集 NDCG 用于早停。保存的 metadata 包含特征顺序、数据摘要、模型参数和最佳迭代。闭环评估从 manifest 重建测试地图，用基线和模型分别运行至终局，保留逐步动作、结束原因和分任务统计。具体首轮结果见 `evaluation/README.md`。

## 特征与状态覆盖对照

```sh
uv run python -m dataset.generate --features-version 2 --output outputs/dataset-v2-features
uv run python xgboost/train.py --dataset outputs/dataset-v2-features --output outputs/xgboost-v2-features
uv run python -m dataset.generate --features-version 2 --actor outputs/xgboost-v2-features --output outputs/dataset-v2-augmented
uv run python xgboost/train.py --dataset outputs/dataset-v2-augmented --output outputs/xgboost-v2-augmented
uv run python -m evaluation.run --model outputs/xgboost-v2-augmented --split validation --output outputs/evaluation-v2-augmented
```

`--actor` 只在训练地图添加模型轨迹，候选标签仍由固定后续策略的分支回报生成。模型保存特征版本，推理时按版本生成相同特征。调参对照使用验证划分，最终盲测需另保留地图。

## 死亡决策诊断与临爆训练起点

```sh
uv run python -m evaluation.diagnose --model outputs/xgboost-v2-augmented --output outputs/diagnosis-v2.json
uv run python -m dataset.generate --features-version 3 --actor outputs/xgboost-v2-features --output outputs/dataset-v3-features
uv run python xgboost/train.py --dataset outputs/dataset-v3-features --output outputs/xgboost-v3-features
uv run python -m dataset.generate --features-version 3 --actor outputs/xgboost-v2-features --threat-starts 4 --output outputs/dataset-v3-threats
uv run python xgboost/train.py --dataset outputs/dataset-v3-threats --output outputs/xgboost-v3-threats
uv run python -m evaluation.run --model outputs/xgboost-v3-threats --split validation --output outputs/evaluation-v3-threats
```

`--threat-starts` 只增加训练地图的临爆起点，记录初始位置、威胁位置和引爆时间。v3 特征在 v2 上增加局部图脱险步数与时间余量，仍只读取已观察格子。

## 固定模型盲测

```sh
uv run python -m evaluation.suite --model outputs/xgboost-v3-threats --episodes 300 --seed-start 3000000 --output outputs/blind-suite-v1
```

案例数必须为 12 的倍数，以平衡三个任务和四种引爆时间。案例在推理前写入 plan.json，验证地图摘要不重叠，并固定模型文件摘要。逐局动作、终止原因以及按任务和引爆时间划分的统计保存在 report.json。已经查看结果的种子范围用于后续回归，新模型选型仍需新的盲测范围。

## 历史特征与时间折扣对照

```sh
uv run python -m dataset.generate --features-version 4 --actor outputs/xgboost-v2-features --threat-starts 4 --output outputs/dataset-v4-history
uv run python xgboost/train.py --dataset outputs/dataset-v4-history --output outputs/xgboost-v4-history
uv run python -m dataset.generate --features-version 4 --actor outputs/xgboost-v2-features --threat-starts 4 --label-version 2 --output outputs/dataset-v4-discount
uv run python xgboost/train.py --dataset outputs/dataset-v4-discount --output outputs/xgboost-v4-discount
uv run python -m evaluation.run --model outputs/xgboost-v4-discount --split validation --output outputs/evaluation-v4-discount
uv run python -m evaluation.suite --model outputs/xgboost-v4-discount --output outputs/regression-v4-discount
uv run python -m evaluation.suite --model outputs/xgboost-v4-discount --seed-start 4000000 --output outputs/blind-suite-v2
```

v4 使用最近 8 步位置及采集进度历史，标签版本 2 使用 0.95 的逐步回报折扣。模型 metadata 携带特征版本，数据 manifest 记录标签版本和折扣。原 300 局种子用于回归，新盲测使用 4000000 起始种子。

## 跟随诊断与任务动作约束

```sh
uv run python -m evaluation.diagnose --model outputs/xgboost-v4-discount --report outputs/blind-suite-v2/report.json --task follow --output outputs/follow-failures-v4.json
uv run python -m evaluation.suite --model outputs/xgboost-v4-discount --seed-start 4000000 --mask-task-actions --output outputs/regression-v4-inference-mask
uv run python -m dataset.generate --features-version 4 --actor outputs/xgboost-v2-features --threat-starts 4 --label-version 2 --task-action-mask --output outputs/dataset-v5-mask
uv run python xgboost/train.py --dataset outputs/dataset-v5-mask --output outputs/xgboost-v5-mask
uv run python -m dataset.generate --features-version 5 --actor outputs/xgboost-v2-features --threat-starts 4 --label-version 2 --task-action-mask --output outputs/dataset-v5-memory
uv run python xgboost/train.py --dataset outputs/dataset-v5-memory --output outputs/xgboost-v5-memory
uv run python -m evaluation.suite --model outputs/xgboost-v5-memory --seed-start 4000000 --output outputs/regression-v5-memory
```

数据生成的 `--task-action-mask` 记录任务约束并用于训练和评估。套件的 `--mask-task-actions` 仅在推理侧增加约束，计划和报告记录最终采用的约束。默认行为保持可复现已有模型。

## 跟随课程

```sh
uv run python -m dataset.generate --features-version 4 --actor outputs/xgboost-v4-discount --threat-starts 4 --label-version 2 --task-action-mask --follow-starts 4 --output outputs/dataset-v6-follow
uv run python xgboost/train.py --dataset outputs/dataset-v6-follow --output outputs/xgboost-v6-follow
uv run python -m dataset.generate --features-version 5 --actor outputs/xgboost-v4-discount --threat-starts 4 --label-version 2 --task-action-mask --follow-starts 4 --output outputs/dataset-v6-memory
uv run python xgboost/train.py --dataset outputs/dataset-v6-memory --output outputs/xgboost-v6-memory
uv run python -m evaluation.follow --model outputs/xgboost-v4-discount --mask-task-actions --output outputs/follow-v4-reference
uv run python -m evaluation.follow --model outputs/xgboost-v6-follow --output outputs/follow-v6-data
uv run python -m evaluation.follow --model outputs/xgboost-v6-memory --output outputs/follow-v6-memory
```

`--follow-starts` 只对训练地图的跟随任务增加场景，包含基线和指定模型的轨迹。初始玩家位置及外部路线记录在数据行中以便复现，模型特征读取受限观察与已观察历史。跟随专项评估使用验证地图，属于开发评估。

## 标签窗口对照

```sh
uv run python -m dataset.generate --features-version 5 --actor outputs/xgboost-v4-discount --threat-starts 4 --label-version 2 --task-action-mask --follow-starts 4 --horizon 32 --output outputs/dataset-v7-horizon32
uv run python xgboost/train.py --dataset outputs/dataset-v7-horizon32 --output outputs/xgboost-v7-horizon32
uv run python -m dataset.compare outputs/dataset-v6-memory outputs/dataset-v7-horizon32
uv run python -m evaluation.follow --model outputs/xgboost-v7-horizon32 --output outputs/follow-v7-horizon32
uv run python -m evaluation.suite --model outputs/xgboost-v7-horizon32 --seed-start 4000000 --output outputs/regression-v7-horizon32
```

compare 命令检查生成配置仅 horizon 不同，且 JSONL 中只有候选标签发生变化，轨迹和特征必须逐行一致。

## Transformer 对照

```sh
uv run python -m transformer.train --dataset outputs/dataset-v6-memory --output outputs/transformer-v1
uv run python -m evaluation.follow --model outputs/transformer-v1 --output outputs/follow-transformer-v1
uv run python -m evaluation.suite --model outputs/transformer-v1 --seed-start 4000000 --output outputs/regression-transformer-v1
```

PyTorch 由 uv 锁定。当前配置采用 CPU 单线程，模型结构和历史输入规则见 transformer/README.md。与同数据 XGBoost 的对照结果见 evaluation/README.md。
