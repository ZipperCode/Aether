<script setup lang="ts">
import { ref } from 'vue'
import {
  Server,
  Key,
  Container,
  Code,
  Shield,
  Monitor,
  Check,
  Copy,
  Zap,
  Search,
  Workflow,
} from 'lucide-vue-next'
import { panelClasses } from './guide-config'

// 部署步骤数据
const activeDeployTab = ref(0)
const copiedStep = ref<string | null>(null)

const productionSteps = [
  {
    title: '克隆代码',
    code: 'git clone https://github.com/ZipperCode/Aether.git\ncd Aether',
    icon: Code
  },
  {
    title: '配置环境变量',
    note: '生成密钥并填入 .env',
    code: 'cp .env.example .env\npython generate_keys.py',
    icon: Key
  },
  {
    title: '部署 / 更新',
    note: '自动执行数据库迁移',
    code: 'docker compose pull && docker compose up -d',
    icon: Container
  },
  {
    title: '升级前备份',
    note: '可选',
    code: 'docker compose exec postgres pg_dump -U postgres aether | gzip > backup_$(date +%Y%m%d_%H%M%S).sql.gz',
    icon: Shield,
    optional: true
  }
]

const localBuildSteps = [
  {
    title: '克隆代码',
    code: 'git clone https://github.com/ZipperCode/Aether.git\ncd Aether',
    icon: Code
  },
  {
    title: '配置环境变量',
    note: '生成密钥并填入 .env',
    code: 'cp .env.example .env\npython generate_keys.py',
    icon: Key
  },
  {
    title: '构建',
    note: '自动构建、启动、迁移',
    code: './deploy.sh',
    icon: Container
  },
  {
    title: '更新',
    note: '需要拉取最新代码',
    code: 'git pull origin master',
    icon: Code,
    optional: true
  }
]

const developmentSteps = [
  {
    title: '启动依赖',
    note: 'PostgreSQL + Redis',
    code: 'docker compose -f docker-compose.build.yml up -d postgres redis',
    icon: Container
  },
  {
    title: '后端',
    note: '热重载开发服务器',
    code: 'uv sync\n./dev.sh',
    icon: Server
  },
  {
    title: '前端',
    note: '自动代理到 8084',
    code: 'cd frontend && npm install && npm run dev',
    icon: Monitor
  }
]

function copyStep(stepId: string, code: string) {
  navigator.clipboard.writeText(code)
  copiedStep.value = stepId
  setTimeout(() => {
    copiedStep.value = null
  }, 2000)
}
</script>

<template>
  <div class="space-y-12">
    <!-- Hero 区域 -->
    <div class="space-y-4">
      <div class="inline-flex items-center gap-1.5 rounded-full bg-[#cc785c]/10 dark:bg-[#cc785c]/20 border border-[#cc785c]/20 dark:border-[#cc785c]/40 px-3 py-1 text-xs font-medium text-[#cc785c] dark:text-[#d4a27f]">
        <Zap class="h-3 w-3" />
        Aether 官方文档
      </div>
      <h1 class="text-3xl font-bold text-[#262624] dark:text-[#f1ead8]">
        快速开始
      </h1>
      <p class="text-base text-[#666663] dark:text-[#a3a094] max-w-2xl">
        本文档将引导您完成 Aether 的项目部署、配置以及反向代理等高级特性的使用。
      </p>
    </div>

    <!-- 1. 项目部署 -->
    <section
      id="production"
      class="scroll-mt-24 lg:scroll-mt-20"
    >
      <h2>1. 项目部署</h2>
      
      <div
        :class="[panelClasses.card]"
        class="mt-6"
      >
        <!-- Tab 切换 -->
        <div class="flex border-b border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] px-5">
          <button
            v-for="(tab, idx) in [
              { icon: Container, label: 'Docker 预构建镜像' },
              { icon: Code, label: '本地代码构建' },
              { icon: Monitor, label: '本地开发' }
            ]"
            :key="idx"
            class="flex items-center gap-2 px-4 py-3 text-sm font-medium whitespace-nowrap transition-colors border-b-2 -mb-px hover:text-[#262624] dark:hover:text-[#f1ead8]"
            :class="activeDeployTab === idx
              ? 'border-[#cc785c] text-[#cc785c] dark:text-[#d4a27f]'
              : 'border-transparent text-[#666663] dark:text-[#a3a094]'"
            @click="activeDeployTab = idx"
          >
            <component
              :is="tab.icon"
              class="h-4 w-4"
            />
            {{ tab.label }}
          </button>
        </div>

        <!-- 生产环境步骤 -->
        <div
          v-show="activeDeployTab === 0"
          class="p-5 space-y-3"
        >
          <div
            v-for="(step, idx) in productionSteps"
            :key="idx"
            class="group rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] overflow-hidden transition-colors"
            :class="step.optional ? 'border-dashed opacity-80' : ''"
          >
            <div class="flex items-center gap-3 px-4 py-3">
              <span class="w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold flex-shrink-0 bg-[#cc785c] text-white">
                {{ idx + 1 }}
              </span>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium text-[#262624] dark:text-[#f1ead8]">{{ step.title }}</span>
                  <span
                    v-if="step.optional"
                    class="text-[10px] px-1.5 py-0.5 rounded-full bg-[#e5e4df] dark:bg-[rgba(227,224,211,0.12)] text-[#666663] dark:text-[#a3a094]"
                  >
                    可选
                  </span>
                </div>
                <span
                  v-if="step.note"
                  class="text-xs text-[#91918d] dark:text-[#a3a094]/80"
                >{{ step.note }}</span>
              </div>
              <button
                class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs text-[#666663] dark:text-[#a3a094] hover:bg-[#f0f0eb] dark:hover:bg-[#3a3731] transition-colors shrink-0"
                @click="copyStep(`prod-${idx}`, step.code)"
              >
                <Check
                  v-if="copiedStep === `prod-${idx}`"
                  class="h-3.5 w-3.5 text-green-500"
                />
                <Copy
                  v-else
                  class="h-3.5 w-3.5"
                />
                {{ copiedStep === `prod-${idx}` ? '已复制' : '复制' }}
              </button>
            </div>
            <pre class="px-4 pb-3 text-[13px] font-mono text-[#262624] dark:text-[#f1ead8] overflow-x-auto leading-relaxed border-t border-[#e5e4df]/50 dark:border-[rgba(227,224,211,0.06)] pt-3 mx-4 mb-1"><code>{{ step.code }}</code></pre>
          </div>
        </div>

        <!-- 本地构建步骤 -->
        <div
          v-show="activeDeployTab === 1"
          class="p-5 space-y-3"
        >
          <div
            v-for="(step, idx) in localBuildSteps"
            :key="idx"
            class="group rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] overflow-hidden transition-colors"
            :class="step.optional ? 'border-dashed opacity-80' : ''"
          >
            <div class="flex items-center gap-3 px-4 py-3">
              <span class="w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold flex-shrink-0 bg-[#cc785c] text-white">
                {{ idx + 1 }}
              </span>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium text-[#262624] dark:text-[#f1ead8]">{{ step.title }}</span>
                  <span
                    v-if="step.optional"
                    class="text-[10px] px-1.5 py-0.5 rounded-full bg-[#e5e4df] dark:bg-[rgba(227,224,211,0.12)] text-[#666663] dark:text-[#a3a094]"
                  >
                    更新时
                  </span>
                </div>
                <span
                  v-if="step.note"
                  class="text-xs text-[#91918d] dark:text-[#a3a094]/80"
                >{{ step.note }}</span>
              </div>
              <button
                class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs text-[#666663] dark:text-[#a3a094] hover:bg-[#f0f0eb] dark:hover:bg-[#3a3731] transition-colors shrink-0"
                @click="copyStep(`build-${idx}`, step.code)"
              >
                <Check
                  v-if="copiedStep === `build-${idx}`"
                  class="h-3.5 w-3.5 text-green-500"
                />
                <Copy
                  v-else
                  class="h-3.5 w-3.5"
                />
                {{ copiedStep === `build-${idx}` ? '已复制' : '复制' }}
              </button>
            </div>
            <pre class="px-4 pb-3 text-[13px] font-mono text-[#262624] dark:text-[#f1ead8] overflow-x-auto leading-relaxed border-t border-[#e5e4df]/50 dark:border-[rgba(227,224,211,0.06)] pt-3 mx-4 mb-1"><code>{{ step.code }}</code></pre>
          </div>
        </div>

        <!-- 开发环境步骤 -->
        <div
          v-show="activeDeployTab === 2"
          class="p-5 space-y-3"
        >
          <div
            v-for="(step, idx) in developmentSteps"
            :key="idx"
            class="group rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] overflow-hidden transition-colors"
          >
            <div class="flex items-center gap-3 px-4 py-3">
              <span class="w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold flex-shrink-0 bg-[#cc785c] text-white">
                {{ idx + 1 }}
              </span>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium text-[#262624] dark:text-[#f1ead8]">{{ step.title }}</span>
                </div>
                <span
                  v-if="step.note"
                  class="text-xs text-[#91918d] dark:text-[#a3a094]/80"
                >{{ step.note }}</span>
              </div>
              <button
                class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs text-[#666663] dark:text-[#a3a094] hover:bg-[#f0f0eb] dark:hover:bg-[#3a3731] transition-colors shrink-0"
                @click="copyStep(`dev-${idx}`, step.code)"
              >
                <Check
                  v-if="copiedStep === `dev-${idx}`"
                  class="h-3.5 w-3.5 text-green-500"
                />
                <Copy
                  v-else
                  class="h-3.5 w-3.5"
                />
                {{ copiedStep === `dev-${idx}` ? '已复制' : '复制' }}
              </button>
            </div>
            <pre class="px-4 pb-3 text-[13px] font-mono text-[#262624] dark:text-[#f1ead8] overflow-x-auto leading-relaxed border-t border-[#e5e4df]/50 dark:border-[rgba(227,224,211,0.06)] pt-3 mx-4 mb-1"><code>{{ step.code }}</code></pre>
          </div>
        </div>
      </div>
    </section>

    <!-- 2. 配置流程 -->
    <section
      id="config-steps"
      class="scroll-mt-24 lg:scroll-mt-20"
    >
      <h2>2. 配置流程</h2>

      <div class="space-y-8 mt-6">
        <div>
          <h3>1. 创建统一模型</h3>
          <p>以 Opus 4.6 为例, 其他模型同样添加即可, 非必要建议只添加官方支持的模型ID。</p>
          <img
            loading="lazy"
            src="/guide/quickstart-create-model.webp"
            alt="创建统一模型"
            class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm mt-4 w-full"
          >
        </div>

        <div>
          <h3>2. 添加提供商</h3>
          <img
            loading="lazy"
            src="/guide/quickstart-add-provider.webp"
            alt="添加提供商"
            class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm mt-4 w-full"
          >
        </div>

        <div>
          <h3>3. 添加端点</h3>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
            <img
              loading="lazy"
              src="/guide/quickstart-add-endpoint-1.webp"
              alt="添加端点 1"
              class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm w-full"
            >
            <img
              loading="lazy"
              src="/guide/quickstart-add-endpoint-2.webp"
              alt="添加端点 2"
              class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm w-full"
            >
          </div>
        </div>

        <div>
          <h3>4. 添加密钥</h3>
          <img
            loading="lazy"
            src="/guide/quickstart-add-key.webp"
            alt="添加密钥"
            class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm mt-4 w-full"
          >
        </div>

        <div>
          <h3>5. 关联全局模型</h3>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
            <img
              loading="lazy"
              src="/guide/quickstart-link-model-1.webp"
              alt="关联全局模型 1"
              class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm w-full"
            >
            <img
              loading="lazy"
              src="/guide/quickstart-link-model-2.webp"
              alt="关联全局模型 2"
              class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm w-full"
            >
          </div>
        </div>

        <div>
          <h3>6. 模型映射</h3>
          <img
            loading="lazy"
            src="/guide/quickstart-model-mapping.webp"
            alt="模型映射"
            class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm mt-4 w-full"
          >
        </div>
      </div>
    </section>

    <!-- 3. 统一搜索入口 -->
    <section
      id="unified-search"
      class="scroll-mt-24 lg:scroll-mt-20"
    >
      <h2>3. 统一搜索入口</h2>
      <p>如果你希望把 Tavily 和 Firecrawl 作为一个团队统一入口暴露出去，而不是让接入方分别对接多个 provider，那么应该启用搜索池网关。</p>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mt-6">
        <div :class="[panelClasses.card]" class="p-5">
          <div class="w-10 h-10 rounded-xl bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 flex items-center justify-center mb-3">
            <Search class="h-5 w-5" />
          </div>
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            统一入口
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            Tavily 负责搜索与提取，Firecrawl 负责抓取。上层应用只需要记住一组网关入口和一个 Bearer Token。
          </p>
        </div>
        <div :class="[panelClasses.card]" class="p-5">
          <div class="w-10 h-10 rounded-xl bg-orange-500/10 text-orange-600 dark:text-orange-400 flex items-center justify-center mb-3">
            <Workflow class="h-5 w-5" />
          </div>
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            统一运维
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            管理端提供服务总览和单服务工作台，用同一套界面管理真实 Key、网关 Token 与额度同步。
          </p>
        </div>
        <div :class="[panelClasses.card]" class="p-5">
          <div class="w-10 h-10 rounded-xl bg-purple-500/10 text-purple-600 dark:text-purple-400 flex items-center justify-center mb-3">
            <Key class="h-5 w-5" />
          </div>
          <h3 class="text-lg font-semibold text-[#262624] dark:text-[#f1ead8] m-0">
            独立鉴权
          </h3>
          <p class="mt-2 text-sm text-[#666663] dark:text-[#a3a094]">
            接入方不直接持有上游真实 Key，而是使用独立生成的网关 Token。这样更适合团队共享和权限隔离。
          </p>
        </div>
      </div>

      <div :class="[panelClasses.card]" class="mt-6 p-6">
        <h3 class="m-0 text-lg font-semibold text-[#262624] dark:text-[#f1ead8]">
          最短接入路径
        </h3>
        <ol class="list-decimal pl-5 mt-4 space-y-3 text-[#666663] dark:text-[#a3a094]">
          <li>进入后台的“搜索池网关”，在总览页选择 Tavily 或 Firecrawl 工作台。</li>
          <li>在工作台里导入真实 Key，必要时点击“同步额度”刷新可用额度。</li>
          <li>创建一个网关 Token，分配给你的脚本、Agent 或内部服务。</li>
          <li>按场景调用统一入口：
            <div class="mt-2 space-y-2 text-sm">
              <div><span class="font-medium text-[#262624] dark:text-[#f1ead8]">POST /api/search</span>：统一搜索</div>
              <div><span class="font-medium text-[#262624] dark:text-[#f1ead8]">POST /api/extract</span>：统一网页提取</div>
              <div><span class="font-medium text-[#262624] dark:text-[#f1ead8]">ANY /firecrawl/*</span>：兼容 Firecrawl 抓取接口</div>
            </div>
          </li>
        </ol>
      </div>
    </section>

    <!-- 4. 反向代理 -->
    <section
      id="reverse-proxy"
      class="scroll-mt-24 lg:scroll-mt-20"
    >
      <h2>4. 反向代理</h2>
      <p>添加提供商时, 提供商类型选择对应类型即可, 反向代理默认开启提供商级格式转换。</p>

      <ul class="list-decimal pl-5 space-y-4 mt-4 text-[#666663] dark:text-[#a3a094]">
        <li>
          <span class="font-medium text-[#262624] dark:text-[#f1ead8]">Codex</span>
          <ul class="list-disc pl-5 mt-2 space-y-1">
            <li>OAuth授权登录</li>
            <li>导入RefreshToken, 支持批量导入</li>
          </ul>
        </li>
        <li>
          <span class="font-medium text-[#262624] dark:text-[#f1ead8]">Kiro</span>
          <ul class="list-disc pl-5 mt-2 space-y-2">
            <li>Build ID</li>
            <li>Identity Center (Start URL, Region)</li>
            <li>
              导入 RefreshToken, 支持批量导入
              <div
                :class="[panelClasses.commandPanel]"
                class="mt-3"
              >
                <div class="px-4 py-2 border-b border-[#e5e4df]/50 dark:border-[rgba(227,224,211,0.06)] bg-[#f5f5f0]/50 dark:bg-[rgba(227,224,211,0.05)] text-xs text-[#91918d] dark:text-[#a3a094]/70">
                  Social 格式要求
                </div>
                <pre class="px-4 py-3 text-[13px] font-mono text-[#262624] dark:text-[#f1ead8] overflow-x-auto m-0 bg-transparent"><code>{
  "refresh_token": ""
}</code></pre>
              </div>
              <div
                :class="[panelClasses.commandPanel]"
                class="mt-3"
              >
                <div class="px-4 py-2 border-b border-[#e5e4df]/50 dark:border-[rgba(227,224,211,0.06)] bg-[#f5f5f0]/50 dark:bg-[rgba(227,224,211,0.05)] text-xs text-[#91918d] dark:text-[#a3a094]/70">
                  IDC 格式要求
                </div>
                <pre class="px-4 py-3 text-[13px] font-mono text-[#262624] dark:text-[#f1ead8] overflow-x-auto m-0 bg-transparent"><code>{
  "refresh_token": "",
  "client_id": "",
  "client_secret": "",
  "machine_id": ""
}</code></pre>
              </div>
            </li>
          </ul>
        </li>
        <li>
          <span class="font-medium text-[#262624] dark:text-[#f1ead8]">Antigravity</span>
          <ul class="list-disc pl-5 mt-2 space-y-1">
            <li>OAuth授权登录</li>
            <li>导入RefreshToken, 支持批量导入</li>
          </ul>
        </li>
      </ul>

      <img
        loading="lazy"
        src="/guide/quickstart-reverse-proxy.webp"
        alt="反向代理配置示例"
        class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] shadow-sm mt-6 w-full max-w-2xl"
      >
    </section>

    <!-- 5. 异步任务 -->
    <section
      id="async-tasks"
      class="scroll-mt-24 lg:scroll-mt-20"
    >
      <h2>5. 异步任务</h2>
      <p>需要有提供商端点支持。</p>
      
      <ul class="list-decimal pl-5 mt-4 text-[#666663] dark:text-[#a3a094] space-y-1">
        <li><span class="font-medium text-[#262624] dark:text-[#f1ead8]">Veo</span></li>
        <li><span class="font-medium text-[#262624] dark:text-[#f1ead8]">Sora</span></li>
      </ul>
    </section>

    <!-- 6. 代理配置 -->
    <section
      id="proxy-config"
      class="scroll-mt-24 lg:scroll-mt-20"
    >
      <h2>6. 代理配置</h2>

      <div class="space-y-6 mt-6">
        <div>
          <h3>1. Aether-Proxy</h3>
          <p>Rust实现, 超小资源占有, 适合性能低的VPS直接使用。</p>
          <a
            href="https://github.com/ZipperCode/Aether/tree/master/aether-proxy"
            target="_blank"
            rel="noopener noreferrer"
            class="text-[#cc785c] dark:text-[#d4a27f] hover:underline mt-2 inline-block"
          >
            GitHub 仓库 >
          </a>
        </div>

        <div>
          <h3>2. 代理节点</h3>
          <p>在模块管理中，开启代理模块后可以添加和使用代理功能，包括手动添加和 Aether-Proxy 自动连接。</p>
        </div>

        <div>
          <h3>3. 多级代理</h3>
          <p>优先级：<span class="text-[#262624] dark:text-[#f1ead8] font-medium bg-[#cc785c]/10 px-2 py-0.5 rounded">Key代理</span> > <span class="text-[#262624] dark:text-[#f1ead8] font-medium bg-[#cc785c]/10 px-2 py-0.5 rounded">提供商代理</span> > <span class="text-[#262624] dark:text-[#f1ead8] font-medium bg-[#cc785c]/10 px-2 py-0.5 rounded">全局代理</span></p>
          
          <ul class="list-decimal pl-5 mt-4 space-y-2 text-[#666663] dark:text-[#a3a094]">
            <li>全局代理 - 系统配置</li>
            <li>提供商代理 - 提供商配置</li>
            <li>Key代理 - Key配置</li>
          </ul>
        </div>
      </div>
    </section>
  </div>
</template>
