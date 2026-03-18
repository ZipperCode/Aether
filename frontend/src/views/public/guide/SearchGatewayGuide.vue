<script setup lang="ts">
import { ref } from 'vue'
import { Check, Copy, KeyRound, Search, Shield, Workflow } from 'lucide-vue-next'
import { panelClasses } from './guide-config'

const copiedBlock = ref<string | null>(null)

function copyBlock(id: string, code: string) {
  navigator.clipboard.writeText(code)
  copiedBlock.value = id
  setTimeout(() => {
    copiedBlock.value = null
  }, 2000)
}

const examples = [
  {
    id: 'search',
    title: '统一搜索',
    description: '适合新闻检索、网页发现、通用搜索场景。',
    code: `curl -X POST https://your-aether.example/api/search \\
  -H "Authorization: Bearer spg-your-token" \\
  -H "Content-Type: application/json" \\
  -d '{
    "query": "latest ai infrastructure news"
  }'`,
  },
  {
    id: 'extract',
    title: '统一提取',
    description: '适合网页正文抓取与结构化提取。',
    code: `curl -X POST https://your-aether.example/api/extract \\
  -H "Authorization: Bearer spg-your-token" \\
  -H "Content-Type: application/json" \\
  -d '{
    "urls": ["https://example.com/article"]
  }'`,
  },
  {
    id: 'firecrawl',
    title: 'Firecrawl 兼容路径',
    description: '适合已有 Firecrawl SDK 或脚本平滑接入。',
    code: `curl -X POST https://your-aether.example/firecrawl/v2/scrape \\
  -H "Authorization: Bearer spg-your-token" \\
  -H "Content-Type: application/json" \\
  -d '{
    "url": "https://example.com"
  }'`,
  },
]
</script>

<template>
  <div class="space-y-12 pb-12">
    <div class="space-y-4">
      <div class="inline-flex items-center gap-1.5 rounded-full bg-emerald-500/10 dark:bg-emerald-500/20 border border-emerald-500/20 dark:border-emerald-500/40 px-3 py-1 text-xs font-medium text-emerald-700 dark:text-emerald-300">
        <Search class="h-3 w-3" />
        Search Gateway
      </div>
      <h1 class="text-3xl font-bold text-[#262624] dark:text-[#f1ead8]">
        统一搜索
      </h1>
      <p class="text-base text-[#666663] dark:text-[#a3a094] max-w-3xl">
        搜索池网关把 Tavily 与 Firecrawl 统一成一组可交付的搜索入口。你可以在后台维护真实 Key 池和网关 Token，然后让 Agent、脚本或内部服务只面对稳定的 Bearer Token 与兼容接口。
      </p>
    </div>

    <section id="why-search-gateway" class="scroll-mt-24 lg:scroll-mt-20">
      <h2>1. 为什么使用统一搜索</h2>
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mt-6">
        <div :class="[panelClasses.card]" class="p-5">
          <div class="w-10 h-10 rounded-xl bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 flex items-center justify-center mb-3">
            <Workflow class="h-5 w-5" />
          </div>
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            一次接入，多种能力
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            用同一套入口覆盖 Tavily 搜索、提取以及 Firecrawl 抓取，不需要让上层业务分别学习不同服务的配置方式。
          </p>
        </div>
        <div :class="[panelClasses.card]" class="p-5">
          <div class="w-10 h-10 rounded-xl bg-orange-500/10 text-orange-600 dark:text-orange-400 flex items-center justify-center mb-3">
            <Shield class="h-5 w-5" />
          </div>
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            不暴露真实 Key
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            接入方只拿网关 Token，不直接接触上游 Key。这样更适合多团队共享，也更容易做轮换和权限边界。
          </p>
        </div>
        <div :class="[panelClasses.card]" class="p-5">
          <div class="w-10 h-10 rounded-xl bg-purple-500/10 text-purple-600 dark:text-purple-400 flex items-center justify-center mb-3">
            <KeyRound class="h-5 w-5" />
          </div>
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            后台统一运维
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            后台先看总览，再进单服务工作台处理真实额度、调用示例、Token 管理和 Key 池维护。
          </p>
        </div>
      </div>
    </section>

    <section id="workspace-setup" class="scroll-mt-24 lg:scroll-mt-20">
      <h2>2. 管理端配置</h2>
      <div :class="[panelClasses.card]" class="mt-6 p-6">
        <ol class="list-decimal pl-5 space-y-4 text-[#666663] dark:text-[#a3a094]">
          <li>
            进入后台 <span class="font-medium text-[#262624] dark:text-[#f1ead8]">搜索池网关</span> 总览页。
          </li>
          <li>
            点击 Tavily 或 Firecrawl 分类卡片，进入对应服务工作台。
          </li>
          <li>
            在工作台的 <span class="font-medium text-[#262624] dark:text-[#f1ead8]">Key 池</span> 中新增或批量导入真实 Key。
          </li>
          <li>
            如需确认额度，点击 <span class="font-medium text-[#262624] dark:text-[#f1ead8]">同步额度</span>，将上游可用额度同步到工作台统计。
          </li>
          <li>
            在 <span class="font-medium text-[#262624] dark:text-[#f1ead8]">Token 池</span> 中创建网关 Token，按需要设置小时、每日、每月限制。
          </li>
          <li>
            将生成的 Token 分配给脚本、Agent、内部服务或网关消费者。
          </li>
        </ol>
      </div>
    </section>

    <section id="access-patterns" class="scroll-mt-24 lg:scroll-mt-20">
      <h2>3. 接入方式</h2>
      <div class="space-y-4 mt-6">
        <div
          v-for="example in examples"
          :key="example.id"
          :class="[panelClasses.card]"
          class="overflow-hidden"
        >
          <div class="flex items-start justify-between gap-4 px-6 py-5 border-b border-[#e5e4df]/50 dark:border-[rgba(227,224,211,0.06)]">
            <div>
              <h3 class="m-0 text-lg font-semibold text-[#262624] dark:text-[#f1ead8]">
                {{ example.title }}
              </h3>
              <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
                {{ example.description }}
              </p>
            </div>
            <button
              class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs text-[#666663] dark:text-[#a3a094] hover:bg-[#f0f0eb] dark:hover:bg-[#3a3731] transition-colors shrink-0"
              @click="copyBlock(example.id, example.code)"
            >
              <Check
                v-if="copiedBlock === example.id"
                class="h-3.5 w-3.5 text-green-500"
              />
              <Copy
                v-else
                class="h-3.5 w-3.5"
              />
              {{ copiedBlock === example.id ? '已复制' : '复制' }}
            </button>
          </div>
          <pre class="px-6 py-5 text-[13px] font-mono text-[#262624] dark:text-[#f1ead8] overflow-x-auto leading-relaxed m-0 bg-transparent"><code>{{ example.code }}</code></pre>
        </div>
      </div>
    </section>

    <section id="troubleshooting" class="scroll-mt-24 lg:scroll-mt-20">
      <h2>4. 常见问题</h2>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-6">
        <div :class="[panelClasses.card]" class="p-5">
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            401 / Invalid token
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            使用了错误的 Bearer Token，或者服务类型不匹配。请确认 Token 是在对应服务工作台下创建的。
          </p>
        </div>
        <div :class="[panelClasses.card]" class="p-5">
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            503 / No available API keys
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            当前服务池没有可用 Key。请检查是否尚未导入、是否全部被停用，或先执行一次额度同步确认状态。
          </p>
        </div>
        <div :class="[panelClasses.card]" class="p-5">
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            为什么要用网关 Token，而不是直接上游 Key？
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            因为统一搜索入口的目标是把真实 Key 留在后台，只对外分发可控、可限额、可轮换的网关 Token。
          </p>
        </div>
        <div :class="[panelClasses.card]" class="p-5">
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            什么时候直接用 provider，什么时候用统一搜索？
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            如果只是单个服务的内部实验，直接 provider 也可以；如果要面向团队、脚本或 Agent 统一交付搜索能力，优先使用统一搜索入口。
          </p>
        </div>
      </div>
    </section>
  </div>
</template>
