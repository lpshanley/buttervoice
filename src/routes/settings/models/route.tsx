import { createFileRoute } from '@tanstack/react-router';
import { VoiceModels } from '../../../components/models/VoiceModels';

export const Route = createFileRoute('/settings/models')({
  component: VoiceModels,
});
