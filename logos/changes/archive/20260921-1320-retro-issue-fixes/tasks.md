# 实现任务（追溯补建：改动已落地，本清单为核对账）

## [delta] 规格变更

无（代码级修复，无规格层变更；#8 无代码改动）

## [code] 代码实现（已落地，逐项核对）

- [x] 批 1（#1#2#3）：WSL binary 文档 + PLACEHOLDER GUARD + REPLACE_ME 拒发校验 + smoke 入口文档（commit 371d9391；测试 12/12+10/10 外环独立重跑一致）
- [x] 批 2（#4#5）：octos-web engines 下限 + npm fallback（8cea119）；hydrate legacy 行修复 + 回归测试（ec44326）；主仓指针（bfde941e）；vitest 3/3 外环独立复现
- [x] 批 3（#7#8）：cluster-worker-profile CM 真相源 + 断言 13/13 外环独立重跑（d284997a）；#8 分类结案（无代码）
- [x] 全部 issue 回帖落地（-R dorname/octos，#1-#8）

## 流程补建（本提案目的）

- [x] 追溯提案创建与文档填写（retroactive 性质声明）
- [x] loop.md 纪律补写（借 guard 窗口：OpenLogos 流程硬约束 + 验收/归档/push 分工 + CPU 保护）
- [ ] merge（no-op delta）→ verify → archive（外环执行，按 operator 通告三分工）
