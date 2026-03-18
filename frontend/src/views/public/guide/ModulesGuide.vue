<script setup lang="ts">
import { Blocks } from 'lucide-vue-next'
import { panelClasses } from './guide-config'
</script>

<template>
  <div class="space-y-12 pb-12">
    <!-- Hero 区域 -->
    <div class="space-y-4">
      <div class="inline-flex items-center gap-1.5 rounded-full bg-[#cc785c]/10 dark:bg-[#cc785c]/20 border border-[#cc785c]/20 dark:border-[#cc785c]/40 px-3 py-1 text-xs font-medium text-[#cc785c] dark:text-[#d4a27f]">
        <Blocks class="h-3 w-3" />
        系统扩展
      </div>
      <h1 class="text-3xl font-bold text-[#262624] dark:text-[#f1ead8]">
        模块管理
      </h1>
      <p class="text-base text-[#666663] dark:text-[#a3a094] max-w-2xl">
        Aether 提供高可插入的模块化管理机制，帮助连接外部服务和授权系统。
      </p>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-6 mt-8">
      <!-- 0. 搜索池网关 -->
      <section
        id="search-pool-gateway"
        :class="[panelClasses.card]"
        class="p-6 scroll-mt-24 lg:scroll-mt-20 flex flex-col md:col-span-2"
      >
        <div class="w-12 h-12 rounded-xl bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 flex items-center justify-center mb-4">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class="lucide lucide-search"
          ><circle
            cx="11"
            cy="11"
            r="8"
          /><path d="m21 21-4.3-4.3" /></svg>
        </div>
        <h3 class="text-xl font-bold text-[#262624] dark:text-[#f1ead8] mb-2 m-0 mt-0 pt-0 border-0">
          搜索池网关
        </h3>
        <p class="text-sm text-[#666663] dark:text-[#a3a094] leading-relaxed">
          将 Tavily 与 Firecrawl 收敛成统一搜索入口。后台提供总览页和单服务工作台，分别管理真实 Key 池、网关 Token 池与额度同步。适合团队内部统一接入搜索、提取与抓取能力，而不是分别维护多个 provider 接口。
        </p>
        <div class="mt-4 grid grid-cols-1 md:grid-cols-3 gap-3">
          <div class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] p-4">
            <div class="text-xs uppercase tracking-[0.14em] text-[#91918d] dark:text-[#a3a094]/80">
              总览页
            </div>
            <div class="mt-2 text-sm font-medium text-[#262624] dark:text-[#f1ead8]">
              先看服务面板
            </div>
            <div class="mt-1 text-sm text-[#666663] dark:text-[#a3a094]">
              快速确认 Tavily / Firecrawl 的活跃 Key、Token 数、今日调用和剩余额度。
            </div>
          </div>
          <div class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] p-4">
            <div class="text-xs uppercase tracking-[0.14em] text-[#91918d] dark:text-[#a3a094]/80">
              工作台
            </div>
            <div class="mt-2 text-sm font-medium text-[#262624] dark:text-[#f1ead8]">
              再进单服务管理
            </div>
            <div class="mt-1 text-sm text-[#666663] dark:text-[#a3a094]">
              在同一页里完成统计查看、调用方式复制、Token 管理、Key 管理和额度同步。
            </div>
          </div>
          <div class="rounded-xl border border-[#e5e4df] dark:border-[rgba(227,224,211,0.12)] p-4">
            <div class="text-xs uppercase tracking-[0.14em] text-[#91918d] dark:text-[#a3a094]/80">
              接入方式
            </div>
            <div class="mt-2 text-sm font-medium text-[#262624] dark:text-[#f1ead8]">
              使用独立 Bearer Token
            </div>
            <div class="mt-1 text-sm text-[#666663] dark:text-[#a3a094]">
              创建网关 Token 后即可通过 `/api/search`、`/api/extract` 和 `/firecrawl/*` 统一访问搜索能力。
            </div>
          </div>
        </div>
      </section>

      <!-- 1. 访问令牌 -->
      <section
        id="management-tokens"
        :class="[panelClasses.card]"
        class="p-6 scroll-mt-24 lg:scroll-mt-20 flex flex-col"
      >
        <div class="w-12 h-12 rounded-xl bg-purple-500/10 text-purple-600 dark:text-purple-400 flex items-center justify-center mb-4">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class="lucide lucide-key-round"
          ><path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z" /><circle
            cx="16.5"
            cy="7.5"
            r=".5"
            fill="currentColor"
          /></svg>
        </div>
        <h3 class="text-xl font-bold text-[#262624] dark:text-[#f1ead8] mb-2 m-0 mt-0 pt-0 border-0">
          1. 访问令牌
        </h3>
        <p class="text-sm text-[#666663] dark:text-[#a3a094] leading-relaxed">
          允许通过 Management Token 授权使用全部或部分后端敏感接口（如自动化运维脚本接入）。
        </p>
      </section>

      <!-- 2. 邮件配置 -->
      <section
        id="email-config"
        :class="[panelClasses.card]"
        class="p-6 scroll-mt-24 lg:scroll-mt-20 flex flex-col"
      >
        <div class="w-12 h-12 rounded-xl bg-blue-500/10 text-blue-600 dark:text-blue-400 flex items-center justify-center mb-4">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class="lucide lucide-mail"
          ><rect
            width="20"
            height="16"
            x="2"
            y="4"
            rx="2"
          /><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" /></svg>
        </div>
        <h3 class="text-xl font-bold text-[#262624] dark:text-[#f1ead8] mb-2 m-0 mt-0 pt-0 border-0">
          2. 邮件配置
        </h3>
        <p class="text-sm text-[#666663] dark:text-[#a3a094] leading-relaxed">
          用于注册验证及账户通知服务。管理员可以自定义 SMTP 服务器并高度定制系统发送的各种 HTML 邮件模板。
        </p>
      </section>

      <!-- 3. OAuth 登录 -->
      <section
        id="oauth-login"
        :class="[panelClasses.card]"
        class="p-6 scroll-mt-24 lg:scroll-mt-20 flex flex-col"
      >
        <div class="w-12 h-12 rounded-xl bg-orange-500/10 text-orange-600 dark:text-orange-400 flex items-center justify-center mb-4">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class="lucide lucide-user-check"
          ><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle
            cx="9"
            cy="7"
            r="4"
          /><polyline points="16 11 18 13 22 9" /></svg>
        </div>
        <h3 class="text-xl font-bold text-[#262624] dark:text-[#f1ead8] mb-2 m-0 mt-0 pt-0 border-0">
          3. OAuth 登录
        </h3>
        <p class="text-sm text-[#666663] dark:text-[#a3a094] leading-relaxed">
          支持包括 Linux Do OAuth 等多种第三方授权验证登录，方便统一认证渠道并提升用户体验。
        </p>
      </section>

      <!-- 4. LDAP 认证 -->
      <section
        id="ldap-auth"
        :class="[panelClasses.card]"
        class="p-6 scroll-mt-24 lg:scroll-mt-20 flex flex-col"
      >
        <div class="w-12 h-12 rounded-xl bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 flex items-center justify-center mb-4">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class="lucide lucide-building-2"
          ><path d="M6 22V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v18Z" /><path d="M6 12H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2" /><path d="M18 9h2a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-2" /><path d="M10 6h4" /><path d="M10 10h4" /><path d="M10 14h4" /><path d="M10 18h4" /></svg>
        </div>
        <h3 class="text-xl font-bold text-[#262624] dark:text-[#f1ead8] mb-2 m-0 mt-0 pt-0 border-0">
          4. LDAP 认证
        </h3>
        <p class="text-sm text-[#666663] dark:text-[#a3a094] leading-relaxed">
          为企业管理构建。直接对接企业现有的 LDAP 或 Active Directory 目录以映射内网员工账户和组织架构。
        </p>
      </section>
    </div>
  </div>
</template>
