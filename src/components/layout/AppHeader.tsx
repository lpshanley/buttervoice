import { useAtomValue } from 'jotai';
import { Link } from '@tanstack/react-router';
import { Terminal } from 'lucide-react';
import {
  Group, Badge, ActionIcon, Tooltip,
} from '@mantine/core';
import {
  setupIssueLabelAtom,
  canOpenDebugTabAtom,
} from '../../stores/app';

export function AppHeader() {
  const setupIssue = useAtomValue(setupIssueLabelAtom);
  const canOpenDebug = useAtomValue(canOpenDebugTabAtom);

  return (
    <Group justify="flex-end" px="lg" h="100%">
      {setupIssue && (
        <Badge color="orange" variant="light" size="sm">
          Action: {setupIssue}
        </Badge>
      )}

      {canOpenDebug && (
        <Tooltip label="Debug">
          <ActionIcon variant="subtle" size="lg" aria-label="Debug" component={Link} to="/debug">
            <Terminal size={18} />
          </ActionIcon>
        </Tooltip>
      )}
    </Group>
  );
}
