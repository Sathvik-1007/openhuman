import { useEffect, useRef, useState } from 'react';

import { useT } from '../../../lib/i18n/I18nContext';
import { getCoreHttpBaseUrl, getCoreRpcToken } from '../../../services/coreRpcClient';
import SettingsHeader from '../components/SettingsHeader';
import { useSettingsNavigation } from '../hooks/useSettingsNavigation';

interface ModelEntry {
  id: string;
  provider: string;
  cost_per_1m_output: number;
  vision: boolean;
  quality_score: number | null;
  hallucination_rate: number | null;
  agents_using: number;
  tasks_evaluated: number;
}

interface HealthConfig {
  hallucination_threshold: number;
  min_tasks_for_rating: number;
  evaluation_window_tasks: number;
}

type SortCol =
  | 'id'
  | 'quality_score'
  | 'hallucination_rate'
  | 'cost_per_1m_output'
  | 'agents_using';
type StatusBadge = 'keep' | 'replace' | 'staging' | 'vision';

function getStatus(m: ModelEntry, cfg: HealthConfig): StatusBadge {
  if (m.vision) return 'vision';
  if (m.tasks_evaluated < cfg.min_tasks_for_rating) return 'staging';
  if (m.hallucination_rate !== null && m.hallucination_rate > cfg.hallucination_threshold)
    return 'replace';
  return 'keep';
}

function qualityStars(score: number | null): string {
  if (score === null) return '—';
  const full = Math.round(score);
  return '★'.repeat(full) + '☆'.repeat(5 - full);
}

const BADGE_STYLES: Record<StatusBadge, { bg: string; text: string; label: string }> = {
  keep: { bg: 'bg-green-500/20', text: 'text-green-400', label: 'settings.modelHealth.badge.keep' },
  replace: {
    bg: 'bg-red-500/20',
    text: 'text-red-400',
    label: 'settings.modelHealth.badge.replace',
  },
  staging: {
    bg: 'bg-amber-500/20',
    text: 'text-amber-400',
    label: 'settings.modelHealth.badge.staging',
  },
  vision: {
    bg: 'bg-blue-500/20',
    text: 'text-blue-400',
    label: 'settings.modelHealth.badge.vision',
  },
};

const ModelHealthPanel = () => {
  const { t } = useT();
  const { navigateBack, breadcrumbs } = useSettingsNavigation();
  const [models, setModels] = useState<ModelEntry[]>([]);
  const [config, setConfig] = useState<HealthConfig>({
    hallucination_threshold: 0.1,
    min_tasks_for_rating: 10,
    evaluation_window_tasks: 50,
  });
  const [sortCol, setSortCol] = useState<SortCol>('id');
  const [sortAsc, setSortAsc] = useState(true);
  const [filterStatus, setFilterStatus] = useState<string>('');
  const [swapTarget, setSwapTarget] = useState<ModelEntry | null>(null);
  const [selectedCandidate, setSelectedCandidate] = useState<ModelEntry | null>(null);
  const [loading, setLoading] = useState(true);
  const fetchedRef = useRef(false);

  useEffect(() => {
    if (fetchedRef.current) return;
    fetchedRef.current = true;
    (async () => {
      try {
        const [baseUrl, token] = await Promise.all([getCoreHttpBaseUrl(), getCoreRpcToken()]);
        if (!token) {
          setLoading(false);
          return;
        }
        const res = await fetch(`${baseUrl}/models/health`, {
          headers: { Authorization: `Bearer ${token}` },
        });
        if (!res.ok) {
          setLoading(false);
          return;
        }
        const data = await res.json();
        if (data.ok) {
          setModels(data.models || []);
          if (data.config) setConfig(data.config);
        }
      } catch {
        /* network error */
      }
      setLoading(false);
    })();
  }, []);

  const handleSort = (col: SortCol) => {
    if (sortCol === col) {
      setSortAsc(!sortAsc);
    } else {
      setSortCol(col);
      setSortAsc(true);
    }
  };

  const sorted = [...models].sort((a, b) => {
    const av = a[sortCol] ?? -1;
    const bv = b[sortCol] ?? -1;
    if (av < bv) return sortAsc ? -1 : 1;
    if (av > bv) return sortAsc ? 1 : -1;
    return 0;
  });

  const filtered = sorted.filter(m => {
    if (!filterStatus) return true;
    return getStatus(m, config) === filterStatus;
  });

  const replaceCandidates = (target: ModelEntry) =>
    models.filter(
      c =>
        c.id !== target.id &&
        !c.vision &&
        (c.hallucination_rate ?? 1) < (target.hallucination_rate ?? 1) &&
        c.cost_per_1m_output <= target.cost_per_1m_output
    );

  const betterCandidates = (target: ModelEntry) =>
    models.filter(
      c =>
        c.id !== target.id &&
        !c.vision &&
        (c.hallucination_rate ?? 1) < (target.hallucination_rate ?? 1) &&
        c.cost_per_1m_output > target.cost_per_1m_output
    );

  const sortIcon = (col: SortCol) => (sortCol === col ? (sortAsc ? ' ↑' : ' ↓') : '');

  return (
    <div data-testid="model-health-panel">
      <SettingsHeader
        title={t('settings.modelHealth.title')}
        showBackButton={true}
        onBack={navigateBack}
        breadcrumbs={breadcrumbs}
      />
      <div className="p-4 space-y-4">
        <div className="flex items-center gap-2 text-xs">
          <select
            className="rounded-lg border border-stone-200 dark:border-neutral-800 bg-stone-50 dark:bg-neutral-800/60 px-3 py-1.5 font-medium text-stone-700 dark:text-neutral-200"
            value={filterStatus}
            onChange={e => setFilterStatus(e.target.value)}>
            <option value="">{t('settings.modelHealth.allStatuses')}</option>
            <option value="keep">{t('settings.modelHealth.badge.keep')}</option>
            <option value="replace">{t('settings.modelHealth.badge.replace')}</option>
            <option value="staging">{t('settings.modelHealth.badge.staging')}</option>
            <option value="vision">{t('settings.modelHealth.badge.vision')}</option>
          </select>
          <span className="text-stone-500 dark:text-neutral-400">
            {filtered.length} {t('settings.modelHealth.models')}
          </span>
        </div>

        {loading ? (
          <p className="text-xs text-stone-400 py-4 text-center">
            {t('settings.modelHealth.loading')}
          </p>
        ) : filtered.length === 0 ? (
          <p className="text-xs text-stone-400 py-4 text-center">
            {t('settings.modelHealth.empty')}
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b border-stone-200 dark:border-neutral-800">
                  <th
                    className="text-left py-2 px-2 cursor-pointer"
                    onClick={() => handleSort('id')}>
                    {t('settings.modelHealth.col.model')}
                    {sortIcon('id')}
                  </th>
                  <th
                    className="text-left py-2 px-2 cursor-pointer"
                    onClick={() => handleSort('quality_score')}>
                    {t('settings.modelHealth.col.quality')}
                    {sortIcon('quality_score')}
                  </th>
                  <th
                    className="text-left py-2 px-2 cursor-pointer"
                    onClick={() => handleSort('hallucination_rate')}>
                    {t('settings.modelHealth.col.halluc')}
                    {sortIcon('hallucination_rate')}
                  </th>
                  <th
                    className="text-left py-2 px-2 cursor-pointer"
                    onClick={() => handleSort('cost_per_1m_output')}>
                    {t('settings.modelHealth.col.cost')}
                    {sortIcon('cost_per_1m_output')}
                  </th>
                  <th
                    className="text-left py-2 px-2 cursor-pointer"
                    onClick={() => handleSort('agents_using')}>
                    {t('settings.modelHealth.col.agents')}
                    {sortIcon('agents_using')}
                  </th>
                  <th className="text-left py-2 px-2">{t('settings.modelHealth.col.status')}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map(m => {
                  const status = getStatus(m, config);
                  const badge = BADGE_STYLES[status];
                  const isReplace = status === 'replace';
                  const candidates = isReplace
                    ? [...replaceCandidates(m), ...betterCandidates(m)]
                    : [];
                  return (
                    <tr
                      key={m.id}
                      className={`border-b border-stone-100 dark:border-neutral-800/50 ${isReplace ? 'bg-red-500/5' : ''}`}>
                      <td className="py-2 px-2">
                        <div className="font-semibold text-stone-900 dark:text-neutral-100">
                          {m.id}
                        </div>
                        <div className="text-[10px] text-stone-400">{m.provider}</div>
                      </td>
                      <td className="py-2 px-2 text-amber-400">{qualityStars(m.quality_score)}</td>
                      <td className="py-2 px-2 font-mono">
                        {m.hallucination_rate !== null ? (
                          <span
                            className={
                              m.hallucination_rate > config.hallucination_threshold
                                ? 'text-red-400'
                                : 'text-green-400'
                            }>
                            {(m.hallucination_rate * 100).toFixed(1)}%
                          </span>
                        ) : (
                          '—'
                        )}
                      </td>
                      <td className="py-2 px-2 font-mono">${m.cost_per_1m_output.toFixed(2)}</td>
                      <td className="py-2 px-2">{m.agents_using}</td>
                      <td className="py-2 px-2">
                        <span
                          className={`rounded-full ${badge.bg} px-2 py-0.5 text-[10px] ${badge.text}`}>
                          {t(badge.label)}
                        </span>
                        {isReplace && candidates.length > 0 && (
                          <button
                            type="button"
                            className="ml-1 text-[10px] text-amber-400 hover:text-amber-300"
                            onClick={() => setSwapTarget(m)}>
                            {t('settings.modelHealth.swap')}
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Swap Modal */}
      {swapTarget && (
        <div
          className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center"
          onClick={() => {
            setSwapTarget(null);
            setSelectedCandidate(null);
          }}>
          <div
            className="bg-white dark:bg-neutral-900 border border-stone-200 dark:border-neutral-700 rounded-xl p-5 max-w-sm w-full mx-4"
            onClick={e => e.stopPropagation()}>
            <h3 className="text-sm font-bold mb-2">{t('settings.modelHealth.modal.title')}</h3>
            <p className="text-xs text-stone-500 dark:text-neutral-400 mb-3">
              {swapTarget.id} — {t('settings.modelHealth.modal.hallucRate')}:{' '}
              {((swapTarget.hallucination_rate ?? 0) * 100).toFixed(1)}%
            </p>
            <div className="space-y-2 mb-4">
              {[...replaceCandidates(swapTarget), ...betterCandidates(swapTarget)].map(c => (
                <div
                  key={c.id}
                  onClick={() => setSelectedCandidate(c)}
                  className={`rounded-lg border p-2 flex items-center justify-between cursor-pointer ${selectedCandidate?.id === c.id ? 'border-green-500 bg-green-500/15' : 'border-green-500/30 bg-green-500/5'}`}>
                  <div>
                    <div className="text-xs font-semibold">{c.id}</div>
                    <div className="text-[10px] text-stone-400">
                      {c.hallucination_rate !== null
                        ? (c.hallucination_rate * 100).toFixed(1)
                        : '?'}
                      % · ${c.cost_per_1m_output.toFixed(2)}/1M
                    </div>
                  </div>
                  <span className="text-[9px] font-bold text-green-400">
                    {c.cost_per_1m_output <= swapTarget.cost_per_1m_output
                      ? t('settings.modelHealth.tag.cheaper')
                      : t('settings.modelHealth.tag.better')}
                  </span>
                </div>
              ))}
            </div>
            <div className="flex gap-2">
              <button
                type="button"
                className="flex-1 py-2 rounded-lg border border-stone-200 dark:border-neutral-700 text-xs"
                onClick={() => {
                  setSwapTarget(null);
                  setSelectedCandidate(null);
                }}>
                {t('settings.modelHealth.modal.cancel')}
              </button>
              <button
                type="button"
                disabled={!selectedCandidate}
                className="flex-1 py-2 rounded-lg bg-blue-600 text-white text-xs font-semibold disabled:opacity-40"
                onClick={() => {
                  setSwapTarget(null);
                  setSelectedCandidate(null);
                }}>
                {t('settings.modelHealth.modal.apply')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default ModelHealthPanel;
