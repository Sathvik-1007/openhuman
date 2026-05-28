import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../../test/test-utils';
import ModelHealthPanel from '../ModelHealthPanel';

vi.mock('../../../../services/coreRpcClient', () => ({
  getCoreHttpBaseUrl: vi.fn().mockResolvedValue('http://localhost:9999'),
  getCoreRpcToken: vi.fn().mockResolvedValue('test-token'),
}));
vi.mock('../../hooks/useSettingsNavigation', () => ({
  useSettingsNavigation: () => ({ navigateBack: vi.fn(), breadcrumbs: [] }),
}));
vi.mock('../../../../lib/i18n/I18nContext', () => ({ useT: () => ({ t: (k: string) => k }) }));

const MOCK_RESPONSE = {
  ok: true,
  models: [
    {
      id: 'deepseek-v3',
      provider: 'SiliconFlow',
      cost_per_1m_output: 0.33,
      vision: false,
      quality_score: 4,
      hallucination_rate: 0.03,
      agents_using: 5,
      tasks_evaluated: 60,
    },
    {
      id: 'qwen-2.5-8b',
      provider: 'OpenRouter',
      cost_per_1m_output: 0.09,
      vision: true,
      quality_score: 3,
      hallucination_rate: 0.04,
      agents_using: 1,
      tasks_evaluated: 20,
    },
    {
      id: 'bad-model',
      provider: 'Test',
      cost_per_1m_output: 1.0,
      vision: false,
      quality_score: 2,
      hallucination_rate: 0.18,
      agents_using: 2,
      tasks_evaluated: 50,
    },
  ],
  config: { hallucination_threshold: 0.1, min_tasks_for_rating: 10, evaluation_window_tasks: 50 },
};

describe('ModelHealthPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders panel with table', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: () => Promise.resolve(MOCK_RESPONSE) });
    renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByText('deepseek-v3')).toBeTruthy();
    });
    expect(screen.getByText('qwen-2.5-8b')).toBeTruthy();
    expect(screen.getByText('bad-model')).toBeTruthy();
  });

  it('shows correct status badges', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: () => Promise.resolve(MOCK_RESPONSE) });
    renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByText('deepseek-v3')).toBeTruthy();
    });
    expect(screen.getAllByText('settings.modelHealth.badge.keep').length).toBeGreaterThan(0);
    expect(screen.getAllByText('settings.modelHealth.badge.vision').length).toBeGreaterThan(0);
    expect(screen.getAllByText('settings.modelHealth.badge.replace').length).toBeGreaterThan(0);
  });

  it('filters by status', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: () => Promise.resolve(MOCK_RESPONSE) });
    const { container } = renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByText('deepseek-v3')).toBeTruthy();
    });
    const select = container.querySelector('select')!;
    fireEvent.change(select, { target: { value: 'vision' } });
    await waitFor(() => {
      expect(screen.queryByText('deepseek-v3')).toBeNull();
    });
    expect(screen.getByText('qwen-2.5-8b')).toBeTruthy();
  });

  it('sorts by column', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: () => Promise.resolve(MOCK_RESPONSE) });
    renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByText('deepseek-v3')).toBeTruthy();
    });
    fireEvent.click(screen.getByText('settings.modelHealth.col.cost'));
  });

  it('shows swap button for replace-flagged models', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: () => Promise.resolve(MOCK_RESPONSE) });
    renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByText('settings.modelHealth.swap')).toBeTruthy();
    });
  });

  it('opens swap modal on click', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: () => Promise.resolve(MOCK_RESPONSE) });
    renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByText('settings.modelHealth.swap')).toBeTruthy();
    });
    fireEvent.click(screen.getByText('settings.modelHealth.swap'));
    await waitFor(() => {
      expect(screen.getByText('settings.modelHealth.modal.title')).toBeTruthy();
    });
  });

  it('shows empty state when no models', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({
        ok: true,
        json: () => Promise.resolve({ ok: true, models: [], config: MOCK_RESPONSE.config }),
      });
    renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByText('settings.modelHealth.empty')).toBeTruthy();
    });
  });

  it('shows loading then content', async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: () => Promise.resolve(MOCK_RESPONSE) });
    renderWithProviders(<ModelHealthPanel />);
    await waitFor(() => {
      expect(screen.getByTestId('model-health-panel')).toBeTruthy();
    });
  });
});
