import { faker } from '@faker-js/faker'

export interface OrganizationRecord {
  id: string
  name: string
  slug: string
  ownerId: string
  createdAt: Date
  updatedAt: Date
}

export class OrganizationFactory {
  static create(overrides: Partial<OrganizationRecord> = {}): OrganizationRecord {
    const name = overrides.name ?? faker.company.name()
    const now = new Date()
    return {
      id: faker.string.uuid(),
      name,
      slug: name.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
      ownerId: faker.string.uuid(),
      createdAt: now,
      updatedAt: now,
      ...overrides,
    }
  }

  static createMany(
    count: number,
    overrides: Partial<OrganizationRecord> = {},
  ): OrganizationRecord[] {
    return Array.from({ length: count }, () => this.create(overrides))
  }
}
