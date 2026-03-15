import { createFileRoute } from '@tanstack/react-router';
import { Box } from '@mantine/core';
import { DebugLogViewer } from '../../components/debug/DebugLogViewer';

export const Route = createFileRoute('/debug')({
  component: () => (
    <Box component="article" h="100%">
      <DebugLogViewer />
    </Box>
  ),
});
