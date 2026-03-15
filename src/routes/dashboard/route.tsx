import { createFileRoute } from '@tanstack/react-router';
import { Box } from '@mantine/core';
import { InfoView } from '../../components/dashboard/InfoView';

export const Route = createFileRoute('/dashboard')({
  component: DashboardPage,
});

function DashboardPage() {
  return (
    <Box p="xl" maw={640} mx="auto">
      <InfoView />
    </Box>
  );
}
