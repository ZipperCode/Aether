<template>
  <PageContainer>
    <PageHeader
      title="签到记录"
      description="站点签到任务的历史记录"
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
        <CheckinHistoryTable />
      </div>
    </Card>
  </PageContainer>
</template>

<script setup lang="ts">
import { PageContainer, PageHeader } from '@/components/layout'
import { Button, Card } from '@/components/ui'
import CheckinHistoryTable from '@/features/site-management/components/CheckinHistoryTable.vue'
</script>
