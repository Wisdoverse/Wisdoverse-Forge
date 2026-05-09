import { faker } from '@faker-js/faker'

export interface UserRecord {
  id: string
  email: string
  username: string
  passwordHash: string
  emailVerified: boolean
  createdAt: Date
  updatedAt: Date
  lastLoginAt: Date | null
  provider: string
  providerId: string | null
  avatarUrl: string | null
}

export class UserFactory {
  static create(overrides: Partial<UserRecord> = {}): UserRecord {
    const now = new Date()
    return {
      id: faker.string.uuid(),
      email: faker.internet.email().toLowerCase(),
      username: faker.internet.username(),
      passwordHash: '$argon2id$v=19$m=65536,t=3,p=4$fakehash',
      emailVerified: true,
      createdAt: now,
      updatedAt: now,
      lastLoginAt: null,
      provider: 'local',
      providerId: null,
      avatarUrl: null,
      ...overrides,
    }
  }

  static createMany(count: number, overrides: Partial<UserRecord> = {}): UserRecord[] {
    return Array.from({ length: count }, () => this.create(overrides))
  }

  static createUnverified(overrides: Partial<UserRecord> = {}): UserRecord {
    return this.create({ emailVerified: false, ...overrides })
  }
}
