import { createFileRoute } from '@tanstack/react-router';
import { PersonaManager } from '../../../components/settings/PersonaManager';

export const Route = createFileRoute('/settings/personas')({
  component: PersonaManager,
});
