# Design

## Source of truth
- Status: Active
- Last refreshed: 2026-07-13
- Primary product surfaces: 管理后台、Provider 管理、Provider Key 状态与配额
- Evidence reviewed: `ProviderDetailDrawer.vue`、`ProviderKeyIdentityBlock.vue`、`ProviderQuotaProgressRow.vue`、`OpenRouterQuotaCard.vue`、`NousQuotaCard.vue`

## Brand
- Personality: 专业、紧凑、可信，面向高密度运维管理
- Trust signals: 明确状态、更新时间、错误原因、可核对的数值
- Avoid: 大面积装饰、孤立纯文本、与现有卡片不一致的渐变和阴影

## Product goals
- Goals: 用户扫一眼即可区分余额、订阅配额、异常状态和剩余比例
- Non-goals: 不把速率限制、消费统计或管理账单伪装成余额
- Success signals: Key 列表无需展开额外页面即可判断可用额度

## Personas and jobs
- Primary personas: 多 Provider、多 Key 的网关管理员
- User jobs: 快速发现余额不足、订阅耗尽、刷新失败和即将重置的账号
- Key contexts of use: 桌面端高密度列表，偶尔在窄屏查看

## Information architecture
- Primary navigation: Provider 管理 -> Provider 详情 -> Endpoint -> Key
- Core routes/screens: Provider Key 列表
- Content hierarchy: Key 身份 > 类型标签 > 核心额度 > 进度/明细 > 重置时间/错误

## Design principles
- 复用现有 Provider Key 卡片、Badge 和进度条语法
- 余额与订阅必须使用不同标题和视觉结构
- 主要数值突出，辅助构成弱化但仍可核对
- Tradeoffs: 优先列表密度与扫读，不展示低价值的原始响应字段

## Visual language
- Color: 使用现有 semantic tokens；正常为 emerald，临界为 amber，耗尽/错误为 red
- Typography: 核心数值 `text-sm font-semibold`，标签与元信息 `text-[9px]` 至 `text-[10px]`
- Spacing/layout rhythm: Key 子卡片使用 `mt-2 p-2`，内部 2-3px 级紧凑间距
- Shape/radius/elevation: `rounded-md bg-muted/30`，不新增阴影
- Motion: 仅保留现有进度条过渡和刷新旋转
- Imagery/iconography: 使用 lucide 小图标辅助余额构成，不用插画

## Components
- Existing components to reuse: Badge、ProviderQuotaSectionHeader、ProviderQuotaProgressRow、PoolKeyQuotaPanel
- New/changed components: ProviderGenericQuotaCard 作为所有 API Key 余额 Provider 的统一卡片；PoolKeyQuotaPanel 复用相同的结构化快照语义
- Variants and states: balance、subscription、loading、error、empty
- Token/component ownership: 沿用 Tailwind 和现有 UI primitives

## Accessibility
- Target standard: 保持现有后台可访问性基线
- Keyboard/focus behavior: 信息卡无新增交互，不制造额外焦点
- Contrast/readability: 状态不能只靠颜色，保留文字和数值
- Screen-reader semantics: 标题、标签和数值保持自然 DOM 顺序
- Reduced motion and sensory considerations: 不新增持续动画

## Responsive behavior
- Supported breakpoints/devices: 桌面优先，窄屏不溢出
- Layout adaptations: 余额明细允许换行，订阅进度保持单列
- Touch/hover differences: 核心信息不依赖 hover

## Interaction states
- Loading: 标题区显示现有旋转刷新图标
- Empty: 不渲染占位卡片
- Error: 红色错误文本，保留最近一次成功数据
- Success: 显示结构化余额或订阅进度
- Disabled: 沿用 Key 行整体禁用透明度
- Offline/slow network, if applicable: 保留缓存快照并显示刷新错误

## Content voice
- Tone: 简洁、明确、可操作
- Terminology: 金额使用“余额”，窗口型资源使用“订阅配额”
- Microcopy rules: 使用“剩余/总额/赠送/充值/重置”，避免“额度信息”等模糊总称

## Implementation constraints
- Framework/styling system: Vue 3、Tailwind、项目 UI primitives
- Design-token constraints: 不新增颜色 token 或组件库
- Performance constraints: Key 列表组件仅做轻量 computed 计算
- Compatibility constraints: 所有 API Key 余额 Provider 使用统一卡片层级；OAuth Provider 原有专用配额语义保持不变
- Test/screenshot expectations: 组件测试覆盖余额、订阅进度和错误状态；通过 typecheck/build

## Open questions
- [ ] 真实 GLM/Z.ai 响应中若出现多个窗口，是否需要按官方优先级排序；当前保持上游顺序
