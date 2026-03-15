import { createFileRoute } from '@tanstack/react-router';
import { StatsDashboard } from '../../../components/stats/StatsDashboard';

export const Route = createFileRoute('/settings/stats')({
  component: StatsDashboard,
});
