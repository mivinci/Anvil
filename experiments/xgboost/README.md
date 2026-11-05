# XGBoost 实验

使用 XGBRanker（rank:pairwise）对每次决策中的有效候选评分。数据按决策分组，固定特征顺序，单线程和固定种子训练，验证集 NDCG 早停。训练参数和数据摘要记录在生成模型的 metadata.json。

从 experiments 运行：

```sh
uv run python xgboost/train.py --dataset outputs/dataset-v1 --output outputs/xgboost-v1
```

输出目录需尚不存在。本目录保存实验脚本，不创建同名 Python 包。
