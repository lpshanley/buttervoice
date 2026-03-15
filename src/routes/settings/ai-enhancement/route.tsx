import { createFileRoute } from '@tanstack/react-router';
import { AiEnhancement } from '../../../components/models/AiEnhancement';

export const Route = createFileRoute('/settings/ai-enhancement')({
  component: AiEnhancement,
});
