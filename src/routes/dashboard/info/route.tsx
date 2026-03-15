import { createFileRoute } from '@tanstack/react-router';
import { InfoView } from '../../../components/dashboard/InfoView';

export const Route = createFileRoute('/dashboard/info')({
  component: InfoView,
});
