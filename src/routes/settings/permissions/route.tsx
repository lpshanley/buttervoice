import { createFileRoute } from '@tanstack/react-router';
import { PermissionsSettings } from '../../../components/settings/PermissionsSettings';

export const Route = createFileRoute('/settings/permissions')({
  component: PermissionsSettings,
});
