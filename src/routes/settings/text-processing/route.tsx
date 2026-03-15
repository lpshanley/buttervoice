import { createFileRoute } from '@tanstack/react-router';
import { TextProcessingSettings } from '../../../components/settings/TextProcessingSettings';

export const Route = createFileRoute('/settings/text-processing')({
  component: TextProcessingSettings,
});
