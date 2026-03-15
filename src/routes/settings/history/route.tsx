import { createFileRoute } from '@tanstack/react-router';
import { HistoryLogViewer } from '../../../components/history/HistoryLogViewer';

export const Route = createFileRoute('/settings/history')({
  component: HistoryLogViewer,
});
