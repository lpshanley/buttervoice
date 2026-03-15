import { createFileRoute } from '@tanstack/react-router';
import { ContentClassification } from '../../../components/settings/ContentClassification';

export const Route = createFileRoute('/settings/content-classification')({
  component: ContentClassification,
});
