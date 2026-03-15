import { createFileRoute } from '@tanstack/react-router';
import { AudioSettings } from '../../../components/settings/AudioSettings';

export const Route = createFileRoute('/settings/audio')({
  component: AudioSettings,
});
