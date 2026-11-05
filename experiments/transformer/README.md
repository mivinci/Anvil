# 小型 Transformer 排序实验

使用同一批候选分组数据，读取最近 8 个已执行候选的状态与动作特征，以及当前所有有效候选。2 层、宽度 64、4 个注意力头，共 80705 参数。历史有时间位置编码，候选不加位置编码，输出对候选顺序保持等变。当前组的标签、实际执行标记与未来结果不进入模型输入。

训练与验证按完整 episode 构造历史，跨 episode 清空。归一化参数只从训练特征拟合，当前候选和历史采用同一变换。使用逐组平均的成对排序损失，平分组不提供成对梯度。CPU 单线程、固定种子，AdamW 学习率 0.0003，最多 30 轮，验证损失连续 6 轮未改善后停止。结果受初始化及训练预算影响，单次实验不足以判断模型结构的最终上限。

从 experiments 运行：

```sh
uv run python -m transformer.train --dataset outputs/dataset-v6-memory --output outputs/transformer-v1
uv run python -m evaluation.follow --model outputs/transformer-v1 --output outputs/follow-transformer-v1
uv run python -m evaluation.suite --model outputs/transformer-v1 --seed-start 4000000 --output outputs/regression-transformer-v1
```

输出为 model.pt 和 metadata.json，记录特征、归一化、数据摘要、训练参数和逐轮验证指标。加载时只读取模型权重。评估入口按 metadata 的模型类型选择实现，每局开始清空历史。生成文件保持在 outputs 下。

32 步标签对照保持相同训练参数：

```sh
uv run python -m transformer.train --dataset outputs/dataset-v7-horizon32 --output outputs/transformer-horizon32
uv run python -m evaluation.follow --model outputs/transformer-horizon32 --output outputs/follow-transformer-horizon32
uv run python -m evaluation.suite --model outputs/transformer-horizon32 --seed-start 4000000 --output outputs/regression-transformer-horizon32
```

本次结果未整体改善，见 evaluation/README.md。
