import { createFileRoute } from '@tanstack/react-router';
import { AdvancedSettingsPage } from '../../../components/settings/AdvancedSettingsPage';

export const Route = createFileRoute('/settings/advanced')({
  component: AdvancedSettingsPage,
});
