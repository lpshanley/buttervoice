import { Outlet } from '@tanstack/react-router';
import { useEffect } from 'react';
import { useAtomValue } from 'jotai';
import { AppShell, Box, Center, Loader, Stack, Text } from '@mantine/core';
import { SettingsNavbar } from './SettingsNavbar';
import { WindowDragRegion } from './WindowDragRegion';
import { OnboardingFlow } from '../onboarding/OnboardingFlow';
import { DictationOverlay } from '../ui/DictationOverlay';
import { loadingAtom, showOnboardingAtom } from '../../stores/app';
import { useAppInit } from '../../lib/hooks/useAppInit';
import { useDictationHudInit } from '../../lib/hooks/useDictationHudInit';

const NAVBAR_WIDTH = 220;
const MAIN_DRAG_REGION_HEIGHT = 32;

function isHudMode(): boolean {
  if (typeof window === 'undefined') return false;
  const params = new URLSearchParams(window.location.search);
  return params.get('hud') === '1';
}

function MainLayout() {
  useAppInit();

  const loading = useAtomValue(loadingAtom);
  const showOnboarding = useAtomValue(showOnboardingAtom);
  const showNavbar = !loading && !showOnboarding;

  return (
    <AppShell
      navbar={{ width: NAVBAR_WIDTH, breakpoint: 'sm', collapsed: { desktop: !showNavbar, mobile: true } }}
      padding={0}
    >
      <AppShell.Navbar>
        <SettingsNavbar />
      </AppShell.Navbar>

      <AppShell.Main style={{ height: '100dvh', overflowY: 'auto' }}>
        <WindowDragRegion
          className="buttervoice-drag-region"
          style={{ height: MAIN_DRAG_REGION_HEIGHT, flexShrink: 0 }}
        />

        <Box style={{ minHeight: `calc(100% - ${MAIN_DRAG_REGION_HEIGHT}px)` }}>
          {loading ? (
            <Center h="100%">
              <Stack align="center" gap="sm">
                <Loader size="sm" />
                <Text size="sm" c="dimmed">Loading ButterVoice…</Text>
              </Stack>
            </Center>
          ) : showOnboarding ? (
            <OnboardingFlow />
          ) : (
            <Outlet />
          )}
        </Box>
      </AppShell.Main>
    </AppShell>
  );
}

function HudLayout() {
  useDictationHudInit();

  useEffect(() => {
    const priorBodyBackground = document.body.style.background;
    const priorHtmlBackground = document.documentElement.style.background;

    document.body.style.background = 'transparent';
    document.documentElement.style.background = 'transparent';

    return () => {
      document.body.style.background = priorBodyBackground;
      document.documentElement.style.background = priorHtmlBackground;
    };
  }, []);

  return <DictationOverlay presentation="window" />;
}

export function RootLayout() {
  return isHudMode() ? <HudLayout /> : <MainLayout />;
}
