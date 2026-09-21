# 实现任务

## [delta] 规格变更
- [x] 无（测试密闭性修复，生产代码零改动）

## [code] 测试修复
- [x] UT-S16-47..51 改用专有变量名 OCTOS_TEST_FB_KEY_23（不可能存在于进程 env）+ 防御性 remove_var
- [x] UT-S16-52 新增密闭性回归：进程 env 注入同名占位污染时回退判定仍只认测试注入的 env_vars
- [x] 生产代码零改动
- [x] 用例登记 core-S16-test-cases.md 批 11 + reporter test-results.jsonl

## 验收
- 导出 ANTHROPIC_API_KEY=sk-real 的 shell 里 profiles:: + runtime::profile:: 全绿
- 本轮不部署
