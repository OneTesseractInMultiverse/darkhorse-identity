const BUILD_VERSION_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$/;

export function resolveBuildVersion({
	override,
	sourceRevision,
	packageVersion
}: {
	override?: string;
	sourceRevision?: string;
	packageVersion: string;
}): string {
	if (override !== undefined) return requireBuildVersion(override);
	if (sourceRevision !== undefined) return requireBuildVersion(sourceRevision);
	return requireBuildVersion(`darkhorse-${packageVersion}`);
}

function requireBuildVersion(value: string): string {
	if (!BUILD_VERSION_PATTERN.test(value)) throw new Error('Invalid Darkhorse build version.');
	return value;
}
