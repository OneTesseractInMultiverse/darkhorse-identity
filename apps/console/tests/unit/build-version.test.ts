import { expect, it } from 'vitest';
import { resolveBuildVersion } from '../../build-version';

it('prefers an explicitly supplied immutable build identifier', () => {
	expect(
		resolveBuildVersion({
			override: 'release-2026.09.26',
			sourceRevision: 'a'.repeat(40),
			packageVersion: '0.1.0'
		})
	).toBe('release-2026.09.26');
});

it('uses the checked-out source revision when no override is supplied', () => {
	expect(
		resolveBuildVersion({
			sourceRevision: 'b'.repeat(40),
			packageVersion: '0.1.0'
		})
	).toBe('b'.repeat(40));
});

it('uses a stable package version when source archives have no Git metadata', () => {
	expect(resolveBuildVersion({ packageVersion: '1.2.3' })).toBe('darkhorse-1.2.3');
});

it('rejects unsafe explicit build identifiers instead of embedding them', () => {
	expect(() =>
		resolveBuildVersion({
			override: 'release\n<bad>',
			packageVersion: '0.1.0'
		})
	).toThrow(/build version/i);
});
