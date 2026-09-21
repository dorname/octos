# 实现任务

## [code] 代码实现
- [ ] ProfileStore：override 路径(`<id>.override.json`)+ get 种子+覆盖合并(来源标记判别)+ save EROFS 改写 override(打 managed_by=ui 标记)
- [ ] UT-S16-36..40(profiles 测试)
- [ ] deploy 清单:override 层 PVC 路径说明(无需新挂载——profiles/ 目录已在 PVC,CM subPath 只覆盖 <id>.json 单文件,override.json 落同目录 PVC 可写)
- [ ] deploy/docs:集群模式 LLM key 三通道(UI 覆盖层/kubectl secret/CM 种子)与优先级
- [ ] 用例登记 core-S16-test-cases.md 批 7 节 + reporter

## 验收(issue #11 四条)
1. UI 配 key 即用(save 落 override,get 合并,dialog 用 cluster-worker 读到 key)
2. rollout 保留(override.json 在 PVC)
3. CM 种子生效(无 override 时)
4. 文档三通道与优先级
