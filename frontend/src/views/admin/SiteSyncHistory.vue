<template>
  <PageContainer>
    <PageHeader
      title="同步记录"
      description="WebDav 源同步的历史记录"
    >
      <template #actions>
        <Button
          variant="ghost"
          @click="$router.push({ name: 'SiteManagement' })"
        >
          &larr; 返回源列表
        </Button>
      </template>
    </PageHeader>

    <Card class="mt-6 overflow-hidden">
      <div class="p-4">
        <SyncHistoryTable />
      </div>
    </Card>
  </PageContainer>
</template>

<script setup lang="ts">
import { PageContainer, PageHeader } from '@/components/layout'
import { Button, Card } from '@/components/ui'
import SyncHistoryTable from '@/features/site-management/components/SyncHistoryTable.vue'
</script>
